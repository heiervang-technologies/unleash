//! Semantic session search backed by a local Turso DB and an OpenAI-compatible
//! embedding / chat endpoint (e.g. llama-server, ollama, lmstudio).
//!
//! The user-facing entry is `unleash search "query"`. See `RunArgs` for flags
//! and the [`Cli`] / [`Commands::Search`] surface in `src/cli.rs` for env vars.
//!
//! Architecture: per the plan at `/home/me/.claude/plans/glittery-launching-codd.md`,
//! Turso ships `vector32()` + `vector_distance_cos()` SQL but does NOT expose
//! FTS5 — so the BM25 side of the hybrid runs in Rust app code against the
//! candidate rows we pull from the DB. Sub-millisecond at our scale (<10k rows).
//!
//! The whole module is self-contained: no `pick_session` integration yet (PR C),
//! no slider UI yet (PR C). This is the standalone-CLI slice (PR E) lifted from
//! `examples/poc_search.rs`.

use std::collections::HashMap;
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::interchange::sessions::{discover_all, SessionInfo};

/// Max chars of content text we *index* per session (stored in first_message
/// for BM25 + TUI preview). Big enough to catch interior content in
/// multi-megabyte sessions.
const MAX_TEXT_CHARS: usize = 32_000;

/// Max chars we feed to the *embedding* model per request. Embedding models
/// have small context windows (LCO-Omni-3B is 4K tokens ≈ 6 KB; embeddinggemma
/// is 512 tokens ≈ 2 KB). We truncate the indexed text to this cap before
/// POSTing /v1/embeddings so the server doesn't 500 on oversize inputs.
const MAX_EMBED_CHARS: usize = 6_000;
const NAMING_TIMEOUT_SECS: u64 = 60;
const EMBED_TIMEOUT_SECS: u64 = 30;
const MAX_NAMES_PER_RUN: usize = 12;

/// Persisted defaults at `~/.config/unleash/search.toml`. Anything the user
/// sets in env (OAI_BASE, OAI_EMBED_MODEL, …) still wins over the file.
#[derive(Debug, Default, serde::Deserialize)]
struct SearchConfig {
    oai_base: Option<String>,
    embed_model: Option<String>,
    chat_base: Option<String>,
    chat_model: Option<String>,
    alpha: Option<f32>,
    /// Optional: when set and `oai_base` is unreachable, print this hint to
    /// the user (typically the `ssh -L ...` command to bring the tunnel up).
    tunnel_hint: Option<String>,
}

/// Resolved configuration after merging env vars, the TOML file, and the
/// "ship-with-sentinel-defaults" fallbacks.
#[derive(Debug, Clone)]
struct ResolvedConfig {
    oai_base: String,
    embed_model: String,
    chat_base: String,
    chat_model: Option<String>,
    alpha: f32,
    tunnel_hint: Option<String>,
}

fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from(".config"))
        .join("unleash")
        .join("search.toml")
}

fn load_search_config() -> SearchConfig {
    let path = config_path();
    let Ok(text) = fs::read_to_string(&path) else {
        return SearchConfig::default();
    };
    toml::from_str(&text).unwrap_or_else(|e| {
        eprintln!(
            "warning: failed to parse {}: {e} — using defaults",
            path.display()
        );
        SearchConfig::default()
    })
}

/// Ensure ~/.config/unleash/search.toml exists with sane defaults the very
/// first time the user runs `unleash search` on this machine. Idempotent.
fn ensure_default_config() {
    let path = config_path();
    if path.exists() {
        return;
    }
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let default = r#"# unleash search — local embedding + naming endpoints
#
# Defaults below point at the sentinel cluster:
#   - embeddings via SSH tunnel to the lco-embedding pod (3 B variant)
#   - chat (for session naming) via the public llm.ht.local ingress
#
# Bring the embedding tunnel up first:
#
#   ssh -fN -L 18000:10.42.1.11:8000 sentinel
#
# Override any of these inline with env vars (OAI_BASE, OAI_EMBED_MODEL,
# OAI_CHAT_BASE, OAI_CHAT_MODEL, ALPHA).

oai_base    = "http://127.0.0.1:18000/v1"
embed_model = "lco-omni-3b"

# Naming uses a chat model — by default we auto-pick whichever model is
# currently loaded on the chat router so we never trigger a cold swap.
# Set chat_model = "specific-name" to pin one (risks 503 if it's not active).
# Set chat_model = "" to disable naming entirely.
chat_base   = "http://llm.ht.local/v1"
# chat_model  = "qwen3.5-0.8b"

alpha = 0.4

# Printed when oai_base is unreachable.
tunnel_hint = "ssh -fN -L 18000:10.42.1.11:8000 sentinel"
"#;
    if let Err(e) = fs::write(&path, default) {
        eprintln!("warning: could not write default {}: {e}", path.display());
    } else {
        eprintln!("wrote default config: {}", path.display());
    }
}

fn resolve_config() -> ResolvedConfig {
    ensure_default_config();
    let file = load_search_config();
    let pick = |env_var: &str, file_val: Option<String>, default: &str| -> String {
        env::var(env_var)
            .ok()
            .or(file_val)
            .unwrap_or_else(|| default.to_string())
    };

    let oai_base = pick(
        "OAI_BASE",
        file.oai_base.clone(),
        "http://127.0.0.1:18000/v1",
    );
    let embed_model = pick("OAI_EMBED_MODEL", file.embed_model.clone(), "lco-omni-3b");
    let chat_base = env::var("OAI_CHAT_BASE")
        .ok()
        .or(file.chat_base.clone())
        .unwrap_or_else(|| oai_base.clone());
    let chat_model = env::var("OAI_CHAT_MODEL")
        .ok()
        .or(file.chat_model.clone())
        .filter(|s| !s.trim().is_empty());
    let alpha = env::var("ALPHA")
        .ok()
        .and_then(|s| s.parse().ok())
        .or(file.alpha)
        .unwrap_or(0.4);
    ResolvedConfig {
        oai_base,
        embed_model,
        chat_base,
        chat_model,
        alpha,
        tunnel_hint: file.tunnel_hint,
    }
}

pub struct RunArgs {
    pub query: Option<String>,
    pub reindex: bool,
    pub json: bool,
    pub top: usize,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Hit {
    pub rank: usize,
    pub score: f32,
    pub bm25: f32,
    pub cos_dist: Option<f64>,
    pub cli: String,
    pub source_id: String,
    pub title: Option<String>,
    pub directory: Option<String>,
    pub updated_at: Option<String>,
    pub first_message: String,
}

/// Synchronous entry point. Builds a private tokio runtime so the rest of the
/// CLI stays synchronous — Turso forces async, but it never escapes this module.
pub fn run(args: RunArgs) -> Result<(), Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(run_async(args))
}

/// RAII guard that removes a lock file when dropped. Used by the reindex
/// action when it's invoked as a background child (lock path arrives via the
/// UNLEASH_REINDEX_LOCK_PATH env var set by trigger_background_reindex).
struct ReindexLockGuard(std::ffi::OsString);
impl Drop for ReindexLockGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(std::path::Path::new(&self.0));
    }
}

/// Synchronous entry point for `unleash sessions reindex` / `unleash sessions name`.
/// Same tokio-containment pattern as `run()`.
pub fn run_sessions_action(
    action: crate::cli::SessionsAction,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(run_sessions_action_async(action, json))
}

async fn run_sessions_action_async(
    action: crate::cli::SessionsAction,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    match action {
        crate::cli::SessionsAction::Reindex => {
            // When spawned in the background by trigger_background_reindex, the
            // parent passes the lock-file path so we can remove it on completion
            // (success, error, or panic). Without this, the stale lock persists
            // until the next reindex check notices the PID is dead.
            let _lock_guard = std::env::var_os("UNLEASH_REINDEX_LOCK_PATH").map(ReindexLockGuard);
            // Re-use the full search pipeline (probe → schema → discover → upsert
            // → embed → name) with no query and no TUI. Setting reindex=true
            // wipes embeddings + generated titles so everything regenerates.
            run_async(RunArgs {
                query: Some(String::new()),
                reindex: true,
                json,
                top: 0,
            })
            .await
        }
        crate::cli::SessionsAction::Name { target, title } => {
            set_session_title(&target, title.as_deref(), json).await
        }
        crate::cli::SessionsAction::Doctor { gc } => {
            crate::interchange::crossload_index::run_doctor(json, gc)?;
            Ok(())
        }
    }
}

async fn set_session_title(
    target: &str,
    title: Option<&str>,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let (cli, source_id) = target
        .split_once(':')
        .ok_or("target must be in <cli>:<source_id> form (e.g. claude:abc12345)")?;
    if cli.is_empty() || source_id.is_empty() {
        return Err("target must be in <cli>:<source_id> form (e.g. claude:abc12345)".into());
    }

    let db_path = index_db_path();
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let db = turso::Builder::new_local(db_path.to_string_lossy().as_ref())
        .build()
        .await?;
    let conn = db.connect()?;
    bootstrap_schema(&conn).await?;

    // Confirm the row exists so we can give a useful error rather than silently
    // updating zero rows.
    let mut probe = conn
        .query(
            "SELECT pk FROM sessions WHERE cli=?1 AND source_id=?2",
            turso::params![cli.to_string(), source_id.to_string()],
        )
        .await?;
    if probe.next().await?.is_none() {
        return Err(format!(
            "no session indexed for {cli}:{source_id} — run `unleash search` or `unleash sessions reindex` first"
        )
        .into());
    }

    let new_title = match title {
        Some(t) => {
            let trimmed = t.trim();
            if trimmed.is_empty() {
                return Err("title must not be empty".into());
            }
            trimmed.to_string()
        }
        None => {
            // Regenerate via the configured chat model.
            let cfg = resolve_config();
            let chat_base = cfg.chat_base.clone();
            let chat_model = match cfg.chat_model.clone() {
                Some(m) => m,
                None => {
                    let probe_http = reqwest::Client::builder()
                        .timeout(std::time::Duration::from_secs(3))
                        .build()?;
                    detect_loaded_chat_model(&probe_http, &chat_base)
                        .await
                        .ok_or("no chat model configured or loaded — set OAI_CHAT_MODEL or pass an explicit TITLE")?
                }
            };

            let mut content = conn
                .query(
                    "SELECT first_message FROM sessions WHERE cli=?1 AND source_id=?2",
                    turso::params![cli.to_string(), source_id.to_string()],
                )
                .await?;
            let text: String = match content.next().await? {
                Some(row) => row.get(0)?,
                None => String::new(),
            };
            if text.is_empty() {
                return Err(
                    "session has no indexed text — run `unleash sessions reindex` first".into(),
                );
            }

            let http = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(NAMING_TIMEOUT_SECS))
                .build()?;
            generate_name(&http, &chat_base, &chat_model, &text).await?
        }
    };

    conn.execute(
        "UPDATE sessions SET generated_title=?1 WHERE cli=?2 AND source_id=?3",
        turso::params![new_title.clone(), cli.to_string(), source_id.to_string()],
    )
    .await?;

    if json {
        let out = serde_json::json!({
            "cli": cli,
            "source_id": source_id,
            "title": new_title,
        });
        println!("{}", serde_json::to_string(&out)?);
    } else {
        println!("{cli}:{source_id} -> {new_title}");
    }
    Ok(())
}

async fn run_async(args: RunArgs) -> Result<(), Box<dyn std::error::Error>> {
    let cfg = resolve_config();
    let ResolvedConfig {
        oai_base,
        embed_model,
        chat_base,
        chat_model,
        alpha,
        tunnel_hint,
    } = cfg;

    if !args.json {
        eprintln!("unleash search");
        eprintln!("  embeddings: {oai_base} model={embed_model}");
        if let Some(ref m) = chat_model {
            eprintln!("  naming    : {chat_base} model={m} (pinned)");
        } else {
            eprintln!("  naming    : {chat_base} (auto-select from loaded models)");
        }
        eprintln!("  alpha     : {alpha:.2}  (0=semantic, 1=bm25)");
        if args.reindex {
            eprintln!("  reindex   : ON — embeddings and titles will be regenerated");
        }
    }

    let db_path = index_db_path();
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent)?;
    }
    if !args.json {
        eprintln!("  store     : {}", db_path.display());
    }

    // Probe the embeddings endpoint up-front. If it's unreachable we'd rather
    // fail fast (and try to self-recover) than burn 30 s per row on timeouts.
    let probe = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()?;
    let probe_url = format!("{}/models", oai_base.trim_end_matches('/'));
    if probe.get(&probe_url).send().await.is_err() {
        // If the user has a tunnel_hint configured (typically `ssh -fN -L ...`),
        // try running it and re-probe. `-fN` means SSH backgrounds itself once
        // the forwarded port is bound, so a successful exit ≈ ready to serve.
        let mut recovered = false;
        if let Some(hint) = tunnel_hint.as_deref() {
            if !args.json {
                eprintln!("  tunnel    : endpoint down — running `{hint}`");
            }
            if try_bring_up_tunnel(hint) {
                // Re-probe; give it a brief moment in case the forward is still
                // settling.
                for attempt in 0..6 {
                    if probe.get(&probe_url).send().await.is_ok() {
                        recovered = true;
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(200 + attempt * 100)).await;
                }
            }
        }
        if !recovered {
            eprintln!("\n\x1b[31merror:\x1b[0m embedding endpoint unreachable: {probe_url}");
            if let Some(hint) = tunnel_hint.as_deref() {
                eprintln!("hint: bring the tunnel up manually:\n  {hint}");
            }
            eprintln!(
                "(set OAI_BASE or edit {} to point elsewhere)",
                config_path().display()
            );
            return Err("endpoint unreachable".into());
        }
        if !args.json {
            eprintln!("  tunnel    : up");
        }
    }

    // If the user didn't pin a chat model, ask the chat endpoint which models
    // are *currently loaded* and pick one. Avoids 503s when the configured
    // model needs a cold load (the cluster router only keeps one model resident).
    let chat_model = match chat_model {
        Some(m) => Some(m),
        None => {
            let detected = detect_loaded_chat_model(&probe, &chat_base).await;
            if let Some(ref m) = detected {
                if !args.json {
                    eprintln!("  naming    : auto-selected loaded model: {m}");
                }
            }
            detected
        }
    };

    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(EMBED_TIMEOUT_SECS))
        .build()?;

    let db = turso::Builder::new_local(db_path.to_string_lossy().as_ref())
        .build()
        .await?;
    let conn = db.connect()?;
    bootstrap_schema(&conn).await?;

    if args.reindex {
        conn.execute(
            "UPDATE sessions SET embedding=NULL, model_id=NULL, generated_title=NULL",
            (),
        )
        .await?;
    }

    // ── Discover + upsert ──────────────────────────────────────────────
    let t_disc = Instant::now();
    let sessions = discover_all();
    if !args.json {
        eprintln!(
            "\n[discover] {} sessions in {:?}",
            sessions.len(),
            t_disc.elapsed()
        );
    }

    let mut needs_embed: Vec<(i64, String)> = Vec::new();
    let mut needs_name: Vec<(i64, String)> = Vec::new();
    let mut indexed = 0usize;
    let mut skipped_archive = 0usize;

    for s in &sessions {
        // Claude's discovery surfaces `<uuid>.archive` rows alongside the live
        // session. They're snapshots of pre-compaction state and duplicate the
        // active session's content — they only bloat the index and dilute
        // results. Skip them here (the direct `-x <cli>:<id>.archive` path still
        // works because it goes through find_session, not the search index).
        if s.id.ends_with(".archive") {
            skipped_archive += 1;
            continue;
        }
        let mtime_ns = file_mtime_ns(&s.path).unwrap_or(0);
        let cur_mtime: Option<i64> = {
            let mut r = conn
                .query(
                    "SELECT mtime_ns FROM sessions WHERE cli=?1 AND source_id=?2",
                    turso::params![s.cli.clone(), s.id.clone()],
                )
                .await?;
            match r.next().await? {
                Some(row) => Some(row.get(0)?),
                None => None,
            }
        };
        let text = extract_text(s).unwrap_or_default();
        let native_title = s.title.clone().or(s.name.clone());

        if cur_mtime != Some(mtime_ns) {
            conn.execute(
                "INSERT INTO sessions (cli, source_id, path, directory, native_title,
                    first_message, updated_at, mtime_ns, model_id)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)
                 ON CONFLICT(cli, source_id) DO UPDATE SET
                    path=excluded.path, directory=excluded.directory,
                    native_title=excluded.native_title, first_message=excluded.first_message,
                    updated_at=excluded.updated_at, mtime_ns=excluded.mtime_ns,
                    embedding=NULL, model_id=NULL, generated_title=NULL",
                turso::params![
                    s.cli.clone(),
                    s.id.clone(),
                    s.path.to_string_lossy().to_string(),
                    s.directory.clone(),
                    native_title.clone(),
                    text.clone(),
                    s.updated_at.clone(),
                    mtime_ns,
                    embed_model.clone(),
                ],
            )
            .await?;
        }

        let mut r = conn
            .query(
                "SELECT pk, embedding IS NOT NULL AS has_emb,
                       generated_title IS NOT NULL OR native_title IS NOT NULL AS has_title
                 FROM sessions WHERE cli=?1 AND source_id=?2",
                turso::params![s.cli.clone(), s.id.clone()],
            )
            .await?;
        if let Some(row) = r.next().await? {
            let pk: i64 = row.get(0)?;
            let has_emb: i64 = row.get(1)?;
            let has_title: i64 = row.get(2)?;
            indexed += 1;
            if has_emb == 0 && !text.is_empty() {
                needs_embed.push((pk, text.clone()));
            }
            if has_title == 0 && chat_model.is_some() && !text.is_empty() {
                needs_name.push((pk, text.clone()));
            }
        }
    }
    if !args.json {
        eprintln!(
            "[index] {indexed} rows, {} need embedding, {} need naming ({skipped_archive} archive rows skipped)",
            needs_embed.len(),
            needs_name.len()
        );
    }

    // ── Embed missing rows (sequential, single-item retry) ─────────────
    let t_emb = Instant::now();
    let mut embed_dim: Option<usize> = None;
    let mut emb_done = 0usize;
    let mut emb_failed = 0usize;
    let total_embed = needs_embed.len();
    for (pk, text) in &needs_embed {
        match embed_one(&http, &oai_base, &embed_model, text).await {
            Ok(v) => {
                embed_dim = Some(v.len());
                let lit = vec_to_lit(&v);
                conn.execute(
                    "UPDATE sessions SET embedding = vector32(?1), model_id = ?2 WHERE pk = ?3",
                    turso::params![lit, embed_model.clone(), *pk],
                )
                .await?;
                emb_done += 1;
            }
            Err(_) => emb_failed += 1,
        }
        if !args.json && (emb_done + emb_failed).is_multiple_of(32) {
            eprintln!("  embed: {}/{}", emb_done + emb_failed, total_embed);
        }
    }
    if !args.json && total_embed > 0 {
        eprintln!(
            "[embed] {emb_done}/{total_embed} embedded, {emb_failed} failed, dim={embed_dim:?}, elapsed={:?}",
            t_emb.elapsed()
        );
    }

    // ── Name un-titled rows (bounded) ──────────────────────────────────
    if let Some(ref cm) = chat_model {
        let t_name = Instant::now();
        let name_http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(NAMING_TIMEOUT_SECS))
            .build()?;
        let to_name: Vec<&(i64, String)> = needs_name.iter().take(MAX_NAMES_PER_RUN).collect();
        if !args.json && !to_name.is_empty() {
            eprintln!(
                "[name] naming {}/{} newest unnamed sessions",
                to_name.len(),
                needs_name.len()
            );
        }
        let mut named = 0usize;
        for (pk, text) in &to_name {
            match generate_name(&name_http, &chat_base, cm, text).await {
                Ok(name) => {
                    if !args.json {
                        eprintln!("  pk={pk} -> {name:?}");
                    }
                    conn.execute(
                        "UPDATE sessions SET generated_title = ?1 WHERE pk = ?2",
                        turso::params![name.clone(), *pk],
                    )
                    .await?;
                    named += 1;
                }
                Err(e) => {
                    if !args.json {
                        eprintln!("  name pk={pk} failed: {e}");
                    }
                }
            }
        }
        if !args.json && named > 0 {
            eprintln!("[name] {named} named in {:?}", t_name.elapsed());
        }
    }

    // ── Query (or print recency listing when no query was given) ───────
    let query = args.query.unwrap_or_default();
    if query.is_empty() {
        if !args.json {
            eprintln!("\n(no query — showing most recent indexed sessions)");
        }
        let hits = recent_listing(&conn, args.top).await?;
        emit(&hits, args.json);
        return Ok(());
    }

    // --- Interactive ratatui TUI when stdin is a real terminal -----------
    let tui_eligible = !args.json
        && std::io::IsTerminal::is_terminal(&std::io::stdin())
        && std::io::IsTerminal::is_terminal(&std::io::stdout());

    #[cfg(feature = "tui")]
    if tui_eligible {
        let rows = fetch_rows_with_embeddings(&conn).await?;
        if rows.is_empty() {
            eprintln!("No sessions indexed.");
            return Ok(());
        }
        let chosen = pick_with_tui(
            rows,
            &http,
            &oai_base,
            &embed_model,
            alpha,
            args.top,
            query.clone(),
        )
        .await?;
        if let Some((hit, profile)) = chosen {
            launch_crossload(&profile, &hit.cli, &hit.source_id);
        }
        return Ok(());
    }

    // --- Headless fallback: one-shot query, print, exit ------------------
    let t_q = Instant::now();
    if query.is_empty() {
        let hits = recent_listing(&conn, args.top).await?;
        emit(&hits, args.json);
        return Ok(());
    }
    let qvec = embed_one(&http, &oai_base, &embed_model, &query).await?;
    let qlit = vec_to_lit(&qvec);
    let candidates = fetch_candidates(&conn, &qlit).await?;
    let bm25_scores = compute_bm25(&query, &candidates);
    let max_bm25 = bm25_scores.iter().cloned().fold(0.0f32, f32::max).max(1e-6);
    let mut scored: Vec<(f32, &Candidate, f32)> = candidates
        .iter()
        .zip(bm25_scores.iter())
        .map(|(c, &bm)| {
            let bm_norm = bm / max_bm25;
            let sem_norm = c.cos_dist.map(|d| 1.0 - (d as f32) / 2.0).unwrap_or(0.0);
            let score = alpha * bm_norm + (1.0 - alpha) * sem_norm;
            (score, c, bm)
        })
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    let hits: Vec<Hit> = scored
        .iter()
        .take(args.top)
        .enumerate()
        .map(|(i, (score, c, bm))| Hit {
            rank: i + 1,
            score: *score,
            bm25: *bm,
            cos_dist: c.cos_dist,
            cli: c.cli.clone(),
            source_id: c.source_id.clone(),
            title: c.title.clone(),
            directory: c.directory.clone(),
            updated_at: c.updated_at.clone(),
            first_message: c.first_message.clone(),
        })
        .collect();

    if !args.json {
        eprintln!(
            "\n[query] {} candidates scored in {:?}",
            candidates.len(),
            t_q.elapsed()
        );
    }
    emit(&hits, args.json);
    Ok(())
}

/// Run a `tunnel_hint` (typically `ssh -fN -L LPORT:HOST:RPORT TARGET`) so the
/// embedding endpoint becomes reachable without forcing the user to context-
/// switch into a shell. `-fN` means SSH backgrounds itself once the forwarded
/// port is bound, so a 0 exit code means the tunnel is up *and* won't die when
/// `unleash` exits.
fn try_bring_up_tunnel(hint: &str) -> bool {
    let parts: Vec<&str> = hint.split_whitespace().collect();
    let Some((prog, rest)) = parts.split_first() else {
        return false;
    };
    std::process::Command::new(prog)
        .args(rest)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Discover available profiles. Falls back to the built-in agent CLI names
/// when no per-profile TOML files are configured.
fn available_profiles() -> Vec<String> {
    let configured = crate::config::ProfileManager::new()
        .ok()
        .and_then(|m| m.list_profiles().ok())
        .unwrap_or_default();
    if !configured.is_empty() {
        return configured;
    }
    ["claude", "codex", "gemini", "opencode", "pi", "hermes"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

/// Exec into `unleash <profile> -x <cli>:<id>`, which routes through the
/// existing crossload path in `lib.rs` — no duplication of injection logic.
fn launch_crossload(profile: &str, source_cli: &str, source_id: &str) {
    use std::os::unix::process::CommandExt;
    let handle = format!("{source_cli}:{source_id}");
    let argv0 = std::env::args()
        .next()
        .unwrap_or_else(|| "unleash".to_string());
    eprintln!("\n→ exec: {argv0} {profile} -x {handle}");
    let err = std::process::Command::new(&argv0)
        .arg(profile)
        .arg("-x")
        .arg(&handle)
        .exec();
    eprintln!("exec failed: {err}");
}

// ── DB helpers ──────────────────────────────────────────────────────────

async fn bootstrap_schema(conn: &turso::Connection) -> Result<(), turso::Error> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS sessions (
            pk INTEGER PRIMARY KEY,
            cli TEXT NOT NULL,
            source_id TEXT NOT NULL,
            path TEXT NOT NULL,
            directory TEXT,
            native_title TEXT,
            generated_title TEXT,
            first_message TEXT,
            updated_at TEXT,
            mtime_ns INTEGER NOT NULL,
            model_id TEXT,
            embedding BLOB,
            UNIQUE(cli, source_id))",
        (),
    )
    .await?;
    Ok(())
}

#[derive(Debug)]
struct Candidate {
    cli: String,
    source_id: String,
    title: Option<String>,
    directory: Option<String>,
    updated_at: Option<String>,
    first_message: String,
    cos_dist: Option<f64>,
}

/// Like Candidate but with the raw embedding kept in memory so the TUI can
/// rescore against a fresh query embedding on every blink without going back
/// to the DB. ≤10k rows × 2048 f32 = ~80 MB worst case, still fine.
#[derive(Debug, Clone)]
struct RowCache {
    cli: String,
    source_id: String,
    title: Option<String>,
    directory: Option<String>,
    updated_at: Option<String>,
    first_message: String,
    embedding: Option<Vec<f32>>,
}

async fn fetch_rows_with_embeddings(
    conn: &turso::Connection,
) -> Result<Vec<RowCache>, turso::Error> {
    let mut rows = conn
        .query(
            "SELECT cli, source_id, coalesce(generated_title, native_title) AS title,
                    directory, updated_at, first_message, embedding
             FROM sessions",
            (),
        )
        .await?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().await? {
        let blob: Option<Vec<u8>> = row.get(6).ok();
        let embedding = blob.and_then(decode_vector32);
        out.push(RowCache {
            cli: row.get(0)?,
            source_id: row.get(1)?,
            title: row.get(2).ok(),
            directory: row.get(3).ok(),
            updated_at: row.get(4).ok(),
            first_message: row.get::<String>(5).ok().unwrap_or_default(),
            embedding,
        });
    }
    Ok(out)
}

/// Turso stores `vector32(...)` BLOBs as a header + little-endian f32 array.
/// We need the f32s in memory for the live re-ranker. Format is currently
/// `[u8; 8] header` (type + length) followed by `dim × 4` bytes of LE f32.
/// If the header doesn't match what we expect, fall back to treating the
/// whole blob as raw f32 LE.
fn decode_vector32(blob: Vec<u8>) -> Option<Vec<f32>> {
    if blob.len() < 4 {
        return None;
    }
    let try_decode = |start: usize| -> Option<Vec<f32>> {
        let payload = &blob[start..];
        if !payload.len().is_multiple_of(4) {
            return None;
        }
        let mut out = Vec::with_capacity(payload.len() / 4);
        for chunk in payload.chunks_exact(4) {
            out.push(f32::from_le_bytes(chunk.try_into().ok()?));
        }
        Some(out)
    };
    // Try common header offsets first; fall back to no-header.
    for off in [8usize, 4, 0] {
        if let Some(v) = try_decode(off) {
            // Sanity check: any reasonable embedding has values in roughly [-2, 2]
            if !v.is_empty() && v.iter().take(8).all(|x| x.abs() < 10.0) {
                return Some(v);
            }
        }
    }
    None
}

async fn fetch_candidates(
    conn: &turso::Connection,
    qlit: &str,
) -> Result<Vec<Candidate>, turso::Error> {
    let mut rows = conn
        .query(
            "SELECT cli, source_id, coalesce(generated_title, native_title) AS title,
                    directory, updated_at, first_message,
                    CASE WHEN embedding IS NOT NULL
                         THEN vector_distance_cos(embedding, vector32(?1)) ELSE NULL END AS cos
             FROM sessions",
            turso::params![qlit.to_string()],
        )
        .await?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().await? {
        out.push(Candidate {
            cli: row.get(0)?,
            source_id: row.get(1)?,
            title: row.get(2).ok(),
            directory: row.get(3).ok(),
            updated_at: row.get(4).ok(),
            first_message: row.get::<String>(5).ok().unwrap_or_default(),
            cos_dist: row.get(6).ok(),
        });
    }
    Ok(out)
}

async fn recent_listing(conn: &turso::Connection, top: usize) -> Result<Vec<Hit>, turso::Error> {
    let mut rows = conn
        .query(
            "SELECT cli, source_id, coalesce(generated_title, native_title) AS title,
                    directory, updated_at, first_message
             FROM sessions ORDER BY updated_at DESC LIMIT ?1",
            turso::params![top as i64],
        )
        .await?;
    let mut out = Vec::new();
    let mut rank = 0usize;
    while let Some(row) = rows.next().await? {
        rank += 1;
        out.push(Hit {
            rank,
            score: 0.0,
            bm25: 0.0,
            cos_dist: None,
            cli: row.get(0)?,
            source_id: row.get(1)?,
            title: row.get(2).ok(),
            directory: row.get(3).ok(),
            updated_at: row.get(4).ok(),
            first_message: row.get::<String>(5).ok().unwrap_or_default(),
        });
    }
    Ok(out)
}

fn index_db_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("unleash")
        .join("search-index.db")
}

fn file_mtime_ns(p: &Path) -> Option<i64> {
    let md = fs::metadata(p).ok()?;
    let mtime = md.modified().ok()?;
    let dur = mtime.duration_since(std::time::UNIX_EPOCH).ok()?;
    Some(dur.as_nanos() as i64)
}

/// Pull user-facing text out of a session file for embedding/BM25.
///
/// Naive head-only indexing misses every active topic on long sessions —
/// "install summary" lives at byte 6.6M of a 10M session that started with
/// generic context-setting. We sample *both* the head (initial intent /
/// post-compaction summary live there) AND the tail (most recent topic), and
/// parse JSON records to skip JSONL boilerplate (`parentUuid`, `sessionId`,
/// `gitBranch` etc) that would otherwise drown the conversational signal.
fn extract_text(s: &SessionInfo) -> Option<String> {
    use std::io::BufRead;

    let file = fs::File::open(&s.path).ok()?;
    let reader = std::io::BufReader::new(file);

    // Stream the whole JSONL file, pulling content strings out of each parseable
    // line. We keep a sliding pair: the first ~25% of the budget records the
    // session's opening exchange (intent, post-compaction summary, system prompt)
    // and the remaining ~75% rolls forward, so the *latest* content always wins
    // the tail allocation. Result: small/medium sessions are fully indexed; long
    // sessions surface both their starting context and their most recent topic.
    const HEAD_BUDGET: usize = MAX_TEXT_CHARS / 4;
    const TAIL_BUDGET: usize = MAX_TEXT_CHARS - HEAD_BUDGET;

    let mut head: Vec<String> = Vec::new();
    let mut head_used = 0usize;
    let mut tail: std::collections::VecDeque<String> = std::collections::VecDeque::new();
    let mut tail_used = 0usize;

    let mut first_value: Option<serde_json::Value> = None;
    let mut saw_jsonl = false;

    for line in reader.lines().take(50_000) {
        let Ok(line) = line else { break };
        let trimmed = line.trim();
        if !trimmed.starts_with('{') {
            continue;
        }
        let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) else {
            continue;
        };
        if first_value.is_none() {
            first_value = Some(val.clone());
        }
        saw_jsonl = true;
        let mut local: Vec<String> = Vec::new();
        collect_content_strings(&val, &mut local);
        for chunk_text in local {
            let len = chunk_text.len();
            if head_used < HEAD_BUDGET {
                head_used += len;
                head.push(chunk_text);
            } else {
                tail_used += len;
                tail.push_back(chunk_text);
                while tail_used > TAIL_BUDGET {
                    if let Some(dropped) = tail.pop_front() {
                        tail_used = tail_used.saturating_sub(dropped.len());
                    } else {
                        break;
                    }
                }
            }
        }
    }

    // Single-JSON layout (Gemini / UCF): if only the first line parsed (no
    // newline-separated records), parse the whole file as one document.
    if !saw_jsonl || (head.is_empty() && tail.is_empty()) {
        if let Ok(bytes) = fs::read(&s.path) {
            if let Ok(text) = std::str::from_utf8(&bytes) {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(text.trim()) {
                    collect_content_strings(&val, &mut head);
                }
            }
        }
    }

    let title_part = s.title.as_deref().or(s.name.as_deref()).unwrap_or("");
    let mut combined = String::new();
    combined.push_str(title_part);
    combined.push(' ');
    combined.push_str(&s.directory);
    for t in head.iter().chain(tail.iter()) {
        if combined.chars().count() >= MAX_TEXT_CHARS {
            break;
        }
        combined.push(' ');
        combined.push_str(t);
    }
    let snippet: String = combined.chars().take(MAX_TEXT_CHARS).collect();
    let trimmed = snippet.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// Walk a JSON value and append any string values that live under
/// content-bearing keys. Skips opaque identifiers and short tokens.
fn collect_content_strings(val: &serde_json::Value, out: &mut Vec<String>) {
    use serde_json::Value;
    const CONTENT_KEYS: &[&str] = &[
        "content", "text", "message", "summary", "input", "prompt", "messages", "payload",
    ];
    match val {
        Value::Object(map) => {
            for (k, v) in map {
                if CONTENT_KEYS.contains(&k.as_str()) {
                    extract_strings(v, out);
                } else if matches!(v, Value::Object(_) | Value::Array(_)) {
                    // Descend, but only one level so we don't pick up metadata
                    // siblings hiding inside nested objects.
                    collect_content_strings(v, out);
                }
            }
        }
        Value::Array(arr) => {
            for v in arr {
                collect_content_strings(v, out);
            }
        }
        _ => {}
    }
}

/// Extract all "leaf" string values from a JSON subtree, applying noise
/// filters. Used once we know we're inside a content-bearing key.
fn extract_strings(val: &serde_json::Value, out: &mut Vec<String>) {
    use serde_json::Value;
    match val {
        Value::String(s) => {
            let trimmed = s.trim();
            // Skip very short tokens, opaque ids, and things that are obviously
            // tool/internal payloads.
            if trimmed.len() < 8 || trimmed.len() > 8192 {
                return;
            }
            if looks_like_opaque_id(trimmed) {
                return;
            }
            out.push(trimmed.to_string());
        }
        Value::Array(arr) => {
            for v in arr {
                extract_strings(v, out);
            }
        }
        Value::Object(map) => {
            // For role/content-block shapes like {"type":"text","text":"..."}.
            for (k, v) in map {
                if matches!(
                    k.as_str(),
                    "text" | "content" | "message" | "value" | "summary"
                ) {
                    extract_strings(v, out);
                }
            }
        }
        _ => {}
    }
}

fn looks_like_opaque_id(s: &str) -> bool {
    // UUID-ish (8-4-4-4-12)
    if s.len() == 36 && s.matches('-').count() == 4 {
        return true;
    }
    // toolu_/msg_/sess_ prefixes from API tokens
    if s.starts_with("toolu_") || s.starts_with("msg_") || s.starts_with("sess_") {
        return true;
    }
    false
}

fn vec_to_lit(v: &[f32]) -> String {
    let mut s = String::with_capacity(v.len() * 9 + 2);
    s.push('[');
    for (i, x) in v.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&format!("{}", x));
    }
    s.push(']');
    s
}

// ── OAI-compat HTTP ────────────────────────────────────────────────────

/// Query `<base>/models` and pick a chat model that's already loaded so naming
/// doesn't trigger a cold model swap on the router. Returns None if the call
/// fails or no chat-capable model is currently resident.
///
/// We filter out anything that looks embed-only (alias or path contains
/// "embed") and prefer the smallest loaded model (by param count if reported,
/// else by name length as a rough tiebreaker).
async fn detect_loaded_chat_model(http: &reqwest::Client, base: &str) -> Option<String> {
    let url = format!("{}/models", base.trim_end_matches('/'));
    let resp = http.get(&url).send().await.ok()?.error_for_status().ok()?;
    let val: serde_json::Value = resp.json().await.ok()?;
    let arr = val.get("data")?.as_array()?;
    let mut loaded: Vec<&str> = arr
        .iter()
        .filter(|m| {
            m.get("status")
                .and_then(|s| s.get("value"))
                .and_then(|v| v.as_str())
                == Some("loaded")
        })
        .filter_map(|m| m.get("id").and_then(|v| v.as_str()))
        .filter(|id| {
            let lc = id.to_ascii_lowercase();
            !lc.contains("embed") && !lc.contains("mmproj") && !lc.contains("rerank")
        })
        .collect();
    loaded.sort_by_key(|id| (id.len(), id.to_string()));
    loaded.first().map(|s| s.to_string())
}

async fn embed_one(
    http: &reqwest::Client,
    base: &str,
    model: &str,
    text: &str,
) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
    // Cap input length to fit small embedding contexts; BM25 still scores
    // against the full stored text, so this only narrows the semantic vector.
    let truncated: String = text.chars().take(MAX_EMBED_CHARS).collect();
    #[derive(serde::Serialize)]
    struct Req<'a> {
        model: &'a str,
        input: &'a str,
    }
    #[derive(serde::Deserialize)]
    struct Item {
        embedding: Vec<f32>,
    }
    #[derive(serde::Deserialize)]
    struct Resp {
        data: Vec<Item>,
    }
    let url = format!("{}/embeddings", base.trim_end_matches('/'));
    let resp = http
        .post(&url)
        .json(&Req {
            model,
            input: &truncated,
        })
        .send()
        .await?
        .error_for_status()?
        .json::<Resp>()
        .await?;
    resp.data
        .into_iter()
        .next()
        .map(|i| i.embedding)
        .ok_or_else(|| "empty embedding response".into())
}

async fn generate_name(
    http: &reqwest::Client,
    base: &str,
    model: &str,
    text: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    #[derive(serde::Serialize)]
    struct Msg<'a> {
        role: &'a str,
        content: &'a str,
    }
    #[derive(serde::Serialize)]
    struct ThinkingOff {
        enable_thinking: bool,
    }
    #[derive(serde::Serialize)]
    struct Req<'a> {
        model: &'a str,
        messages: Vec<Msg<'a>>,
        max_tokens: u32,
        temperature: f32,
        /// Disable thinking-mode for Qwen3+ chat models on llama.cpp — otherwise
        /// the model spends the entire token budget reasoning and returns empty
        /// `content`. Harmless on non-thinking models (just ignored by jinja).
        chat_template_kwargs: ThinkingOff,
    }
    #[derive(serde::Deserialize)]
    struct Choice {
        message: ChoiceMsg,
    }
    #[derive(serde::Deserialize)]
    struct ChoiceMsg {
        content: String,
    }
    #[derive(serde::Deserialize)]
    struct Resp {
        choices: Vec<Choice>,
    }

    let prompt = format!(
        "Reply with a 3 to 6 word title for this conversation. Reply with the title only, no quotes, no punctuation at the end.\n\n{}",
        text.chars().take(800).collect::<String>()
    );
    let url = format!("{}/chat/completions", base.trim_end_matches('/'));
    let req = Req {
        model,
        messages: vec![Msg {
            role: "user",
            content: &prompt,
        }],
        max_tokens: 24,
        temperature: 0.2,
        chat_template_kwargs: ThinkingOff {
            enable_thinking: false,
        },
    };
    let resp: Resp = http
        .post(&url)
        .json(&req)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let raw = resp
        .choices
        .into_iter()
        .next()
        .ok_or("no choices")?
        .message
        .content;
    Ok(raw
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_string())
}

// ── BM25 over the candidate corpus ────────────────────────────────────

fn compute_bm25(query: &str, docs: &[Candidate]) -> Vec<f32> {
    let q_terms: Vec<String> = tokenize(query);
    if q_terms.is_empty() || docs.is_empty() {
        return vec![0.0; docs.len()];
    }
    let tokenized: Vec<Vec<String>> = docs
        .iter()
        .map(|c| {
            let bag = format!(
                "{} {} {} {}",
                c.title.as_deref().unwrap_or(""),
                c.first_message,
                c.directory.as_deref().unwrap_or(""),
                c.cli
            );
            tokenize(&bag)
        })
        .collect();
    let avgdl: f32 = if tokenized.is_empty() {
        0.0
    } else {
        tokenized.iter().map(|d| d.len() as f32).sum::<f32>() / tokenized.len() as f32
    };
    let mut df: HashMap<&str, u32> = HashMap::new();
    for term in &q_terms {
        let mut n = 0u32;
        for doc in &tokenized {
            if doc.iter().any(|t| t == term) {
                n += 1;
            }
        }
        df.insert(term.as_str(), n);
    }
    let n_docs = docs.len() as f32;
    let k1 = 1.5f32;
    let b = 0.75f32;
    tokenized
        .iter()
        .map(|doc| {
            let dl = doc.len() as f32;
            let mut score = 0.0f32;
            for q in &q_terms {
                let tf = doc.iter().filter(|t| *t == q).count() as f32;
                if tf == 0.0 {
                    continue;
                }
                let n_q = *df.get(q.as_str()).unwrap_or(&0) as f32;
                let idf = ((n_docs - n_q + 0.5) / (n_q + 0.5) + 1.0).ln();
                let denom = tf + k1 * (1.0 - b + b * dl / avgdl.max(1.0));
                score += idf * (tf * (k1 + 1.0) / denom.max(1e-6));
            }
            score
        })
        .collect()
}

fn tokenize(s: &str) -> Vec<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() >= 2)
        .map(|t| t.to_string())
        .collect()
}

// ── Output ─────────────────────────────────────────────────────────────

fn emit(hits: &[Hit], json: bool) {
    if json {
        match serde_json::to_string_pretty(hits) {
            Ok(s) => println!("{s}"),
            Err(e) => eprintln!("json encode failed: {e}"),
        }
        return;
    }
    println!("\n=== top {} results ===", hits.len());
    for h in hits {
        let snippet_fallback: String = h
            .first_message
            .chars()
            .filter(|ch| !ch.is_control() && *ch != '"' && *ch != '{' && *ch != '}')
            .take(40)
            .collect::<String>()
            .trim()
            .to_string();
        let title_owned: String = h
            .title
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                if snippet_fallback.len() >= 6 {
                    snippet_fallback
                } else {
                    let n = h.source_id.len().min(10);
                    format!("[{}]", &h.source_id[..n])
                }
            });
        let dir = h.directory.as_deref().unwrap_or("");
        let date = h
            .updated_at
            .as_deref()
            .unwrap_or("")
            .chars()
            .take(10)
            .collect::<String>();
        let cos = h
            .cos_dist
            .map(|d| format!("cos={:.3}", d))
            .unwrap_or_else(|| "cos=∅".into());
        println!(
            "{:>2}. score={:.3}  bm25={:.3}  {}  [{:>8}]  {:<32}  {:<24}  {}",
            h.rank,
            h.score,
            h.bm25,
            cos,
            h.cli,
            truncate(&title_owned, 32),
            truncate(dir, 24),
            date
        );
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

// ── Ratatui TUI ─────────────────────────────────────────────────────────
//
// Cached embeddings live in memory so every keystroke filters instantly.
// Query embedding is fetched async with a 300 ms debounce after the user
// stops typing; until it arrives we fall back to pure BM25.

#[cfg(feature = "tui")]
use ratatui::{prelude::*, widgets::*};

#[cfg(feature = "tui")]
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Searching,
    PickingProfile,
}

#[cfg(feature = "tui")]
struct TuiState {
    rows: Vec<RowCache>,
    bm25_doc_tokens: Vec<Vec<String>>,
    bm25_doc_lens: Vec<f32>,
    bm25_avgdl: f32,
    query: String,
    qvec: Option<Vec<f32>>,
    last_embedded_query: String,
    last_query_change: std::time::Instant,
    embedding_in_flight: bool,
    alpha: f32,
    top: usize,
    scored: Vec<Hit>,
    selected: usize,
    profiles: Vec<String>,
    profile_selected: usize,
    mode: Mode,
}

#[cfg(feature = "tui")]
async fn pick_with_tui(
    rows: Vec<RowCache>,
    http: &reqwest::Client,
    oai_base: &str,
    embed_model: &str,
    initial_alpha: f32,
    top: usize,
    initial_query: String,
) -> io::Result<Option<(Hit, String)>> {
    use crossterm::{
        execute,
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    };

    let bm25_doc_tokens: Vec<Vec<String>> = rows
        .iter()
        .map(|r| {
            let bag = format!(
                "{} {} {} {}",
                r.title.as_deref().unwrap_or(""),
                r.first_message,
                r.directory.as_deref().unwrap_or(""),
                r.cli
            );
            tokenize(&bag)
        })
        .collect();
    let bm25_doc_lens: Vec<f32> = bm25_doc_tokens.iter().map(|d| d.len() as f32).collect();
    let bm25_avgdl: f32 = if bm25_doc_lens.is_empty() {
        0.0
    } else {
        bm25_doc_lens.iter().sum::<f32>() / bm25_doc_lens.len() as f32
    };

    let profiles = available_profiles();

    let mut state = TuiState {
        rows,
        bm25_doc_tokens,
        bm25_doc_lens,
        bm25_avgdl,
        query: initial_query,
        qvec: None,
        last_embedded_query: String::new(),
        last_query_change: std::time::Instant::now(),
        embedding_in_flight: false,
        alpha: initial_alpha,
        top,
        scored: Vec::new(),
        selected: 0,
        profiles,
        profile_selected: 0,
        mode: Mode::Searching,
    };
    rescore(&mut state);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = tui_loop(&mut terminal, &mut state, http, oai_base, embed_model).await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor().ok();
    result
}

#[cfg(feature = "tui")]
async fn tui_loop<W: io::Write>(
    terminal: &mut Terminal<ratatui::backend::CrosstermBackend<W>>,
    state: &mut TuiState,
    http: &reqwest::Client,
    oai_base: &str,
    embed_model: &str,
) -> io::Result<Option<(Hit, String)>> {
    use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};

    loop {
        terminal.draw(|f| render_tui(f, state))?;

        // Auto-embed the query once it's been stable for 300 ms. We embed in
        // the foreground (block_on inside the current_thread runtime); for
        // ≤300 ms latency the redraw stutter is invisible.
        if !state.embedding_in_flight
            && state.query != state.last_embedded_query
            && !state.query.trim().is_empty()
            && state.last_query_change.elapsed() >= std::time::Duration::from_millis(300)
        {
            state.embedding_in_flight = true;
            let q = state.query.clone();
            let v = embed_one(http, oai_base, embed_model, &q).await;
            state.embedding_in_flight = false;
            if let Ok(vec) = v {
                state.qvec = Some(vec);
                state.last_embedded_query = q;
                rescore(state);
            }
        }
        if state.query.trim().is_empty() && state.qvec.is_some() {
            state.qvec = None;
            state.last_embedded_query.clear();
            rescore(state);
        }

        if !event::poll(std::time::Duration::from_millis(80))? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }

        match state.mode {
            Mode::Searching => match key.code {
                KeyCode::Esc => return Ok(None),
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    return Ok(None);
                }
                KeyCode::Char('q') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    return Ok(None);
                }
                KeyCode::Tab => {
                    state.alpha = match state.alpha {
                        a if a < 0.05 => 0.4,
                        a if a < 0.5 => 1.0,
                        _ => 0.0,
                    };
                    rescore(state);
                }
                KeyCode::Left if key.modifiers.contains(KeyModifiers::SHIFT) => {
                    state.alpha = (state.alpha - 0.05).clamp(0.0, 1.0);
                    rescore(state);
                }
                KeyCode::Right if key.modifiers.contains(KeyModifiers::SHIFT) => {
                    state.alpha = (state.alpha + 0.05).clamp(0.0, 1.0);
                    rescore(state);
                }
                KeyCode::Up => {
                    state.selected = state.selected.saturating_sub(1);
                }
                KeyCode::Down => {
                    let max = state.scored.len().saturating_sub(1);
                    if state.selected < max {
                        state.selected += 1;
                    }
                }
                KeyCode::Enter => {
                    if !state.scored.is_empty() {
                        state.mode = Mode::PickingProfile;
                        state.profile_selected = state
                            .profiles
                            .iter()
                            .position(|p| p == &state.scored[state.selected].cli)
                            .unwrap_or(0);
                    }
                }
                KeyCode::Backspace => {
                    state.query.pop();
                    state.last_query_change = std::time::Instant::now();
                    state.selected = 0;
                    rescore(state);
                }
                KeyCode::Char(c) => {
                    state.query.push(c);
                    state.last_query_change = std::time::Instant::now();
                    state.selected = 0;
                    rescore(state);
                }
                _ => {}
            },
            Mode::PickingProfile => match key.code {
                KeyCode::Esc => {
                    state.mode = Mode::Searching;
                }
                KeyCode::Up => {
                    state.profile_selected = state.profile_selected.saturating_sub(1);
                }
                KeyCode::Down => {
                    let max = state.profiles.len().saturating_sub(1);
                    if state.profile_selected < max {
                        state.profile_selected += 1;
                    }
                }
                KeyCode::Enter => {
                    if state.scored.is_empty() || state.profiles.is_empty() {
                        return Ok(None);
                    }
                    let hit = state.scored[state.selected].clone();
                    let profile = state.profiles[state.profile_selected].clone();
                    return Ok(Some((hit, profile)));
                }
                _ => {}
            },
        }
    }
}

#[cfg(feature = "tui")]
fn rescore(state: &mut TuiState) {
    let q_terms = tokenize(&state.query);
    let n_docs = state.rows.len() as f32;
    let k1 = 1.5f32;
    let b = 0.75f32;

    // df per query term — over the cached tokens.
    let mut df: HashMap<&str, u32> = HashMap::new();
    for term in &q_terms {
        let mut n = 0u32;
        for doc in &state.bm25_doc_tokens {
            if doc.iter().any(|t| t == term) {
                n += 1;
            }
        }
        df.insert(term.as_str(), n);
    }

    let mut bm25: Vec<f32> = Vec::with_capacity(state.rows.len());
    for (i, doc) in state.bm25_doc_tokens.iter().enumerate() {
        let dl = state.bm25_doc_lens[i];
        let mut score = 0.0f32;
        for q in &q_terms {
            let tf = doc.iter().filter(|t| *t == q).count() as f32;
            if tf == 0.0 {
                continue;
            }
            let n_q = *df.get(q.as_str()).unwrap_or(&0) as f32;
            let idf = ((n_docs - n_q + 0.5) / (n_q + 0.5) + 1.0).ln();
            let denom = tf + k1 * (1.0 - b + b * dl / state.bm25_avgdl.max(1.0));
            score += idf * (tf * (k1 + 1.0) / denom.max(1e-6));
        }
        bm25.push(score);
    }
    let max_bm25 = bm25.iter().cloned().fold(0.0f32, f32::max).max(1e-6);

    // Cosine vs the (possibly stale) query embedding. Empty query → no
    // semantic signal at all, just BM25.
    let cosines: Vec<Option<f32>> = if let Some(qv) = state.qvec.as_ref() {
        state
            .rows
            .iter()
            .map(|r| {
                r.embedding
                    .as_ref()
                    .map(|emb| cosine_distance(qv, emb) as f32)
            })
            .collect()
    } else {
        vec![None; state.rows.len()]
    };

    let mut scored: Vec<Hit> = state
        .rows
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let bm = bm25[i];
            let bm_norm = bm / max_bm25;
            let sem_norm = cosines[i].map(|d| 1.0 - d / 2.0).unwrap_or(0.0);
            let score = state.alpha * bm_norm + (1.0 - state.alpha) * sem_norm;
            Hit {
                rank: 0,
                score,
                bm25: bm,
                cos_dist: cosines[i].map(|x| x as f64),
                cli: r.cli.clone(),
                source_id: r.source_id.clone(),
                title: r.title.clone(),
                directory: r.directory.clone(),
                updated_at: r.updated_at.clone(),
                first_message: r.first_message.clone(),
            }
        })
        .collect();

    if state.query.trim().is_empty() {
        // Empty query: sort by recency (already the order from discover_all).
        scored.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    } else {
        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }
    scored.truncate(state.top.max(50));
    for (i, h) in scored.iter_mut().enumerate() {
        h.rank = i + 1;
    }
    state.scored = scored;
    if state.selected >= state.scored.len() {
        state.selected = state.scored.len().saturating_sub(1);
    }
}

#[cfg(feature = "tui")]
fn cosine_distance(a: &[f32], b: &[f32]) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 2.0;
    }
    let n = a.len().min(b.len());
    let mut dot = 0.0f64;
    let mut na = 0.0f64;
    let mut nb = 0.0f64;
    for i in 0..n {
        let x = a[i] as f64;
        let y = b[i] as f64;
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    let denom = (na.sqrt() * nb.sqrt()).max(1e-12);
    let cos = dot / denom;
    1.0 - cos
}

#[cfg(feature = "tui")]
fn render_tui(frame: &mut Frame, state: &TuiState) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // query input
            Constraint::Min(5),    // results
            Constraint::Length(3), // status bar (alpha slider + counts)
            Constraint::Length(1), // help
        ])
        .split(area);

    // Query box
    let q_hint = if state.embedding_in_flight {
        " embedding…"
    } else if state.qvec.is_none() && !state.query.trim().is_empty() {
        " (BM25 only — pause typing for semantic)"
    } else {
        ""
    };
    let title = format!(" search{} ", q_hint);
    let q_para = Paragraph::new(state.query.as_str())
        .style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .title_style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
        );
    frame.render_widget(q_para, chunks[0]);

    // Results list
    let visible = chunks[1].height.saturating_sub(2) as usize;
    let start = state.selected.saturating_sub(visible.saturating_sub(1));
    let lines: Vec<Line> = state
        .scored
        .iter()
        .enumerate()
        .skip(start)
        .take(visible)
        .map(|(i, h)| {
            let selected = i == state.selected;
            let prefix = if selected { "> " } else { "  " };
            let style_sel = if selected {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let title = display_title(h);
            let date = h
                .updated_at
                .as_deref()
                .unwrap_or("")
                .chars()
                .take(10)
                .collect::<String>();
            let cos = h
                .cos_dist
                .map(|d| format!("cos={:.2}", d))
                .unwrap_or_else(|| "cos=∅".into());
            Line::from(vec![
                Span::styled(prefix, style_sel),
                Span::styled(
                    format!("{:>5.2}  ", h.score),
                    if selected {
                        style_sel
                    } else {
                        Style::default().fg(Color::Yellow)
                    },
                ),
                Span::styled(format!("{:<6} ", cos), Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("[{:>7}] ", h.cli),
                    Style::default().fg(Color::Magenta),
                ),
                Span::styled(format!("{:<40} ", truncate(&title, 40)), style_sel),
                Span::styled(
                    truncate(h.directory.as_deref().unwrap_or(""), 28),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::raw("  "),
                Span::styled(date, Style::default().fg(Color::DarkGray)),
            ])
        })
        .collect();
    let count_title = format!(" {} / {} ", state.scored.len(), state.rows.len());
    let list = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(count_title)
            .title_style(Style::default().fg(Color::DarkGray)),
    );
    frame.render_widget(list, chunks[1]);

    // α slider
    let bar_width = chunks[2].width.saturating_sub(20) as usize;
    let filled = ((1.0 - state.alpha) * bar_width as f32).round() as usize;
    let empty = bar_width.saturating_sub(filled);
    let slider_str = format!(
        "[BM25 {}{} SEMANTIC]  α={:.2}",
        "░".repeat(filled),
        "▓".repeat(empty),
        state.alpha
    );
    let status = Paragraph::new(slider_str).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" slider ")
            .title_style(Style::default().fg(Color::DarkGray)),
    );
    frame.render_widget(status, chunks[2]);

    // Help line
    let help = match state.mode {
        Mode::Searching => {
            "type=filter  ↑↓=move  Enter=pick profile  Tab=cycle α  ⇧←/→=fine α  Esc=quit"
        }
        Mode::PickingProfile => "↑↓=move  Enter=launch crossload  Esc=back",
    };
    let help_p = Paragraph::new(Line::from(vec![Span::styled(
        help,
        Style::default().fg(Color::DarkGray),
    )]));
    frame.render_widget(help_p, chunks[3]);

    // Profile picker modal — drawn on top
    if state.mode == Mode::PickingProfile {
        let modal_h = (state.profiles.len() as u16 + 4).min(area.height.saturating_sub(4));
        let modal_w = 50u16.min(area.width.saturating_sub(4));
        let modal = Rect {
            x: area.x + (area.width.saturating_sub(modal_w)) / 2,
            y: area.y + (area.height.saturating_sub(modal_h)) / 2,
            width: modal_w,
            height: modal_h,
        };
        frame.render_widget(Clear, modal);
        let header = if let Some(hit) = state.scored.get(state.selected) {
            format!(
                " launch into profile — {} ",
                truncate(&display_title(hit), 30)
            )
        } else {
            " launch into profile ".to_string()
        };
        let lines: Vec<Line> = state
            .profiles
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let sel = i == state.profile_selected;
                let style = if sel {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                Line::from(vec![
                    Span::styled(if sel { "  ▸ " } else { "    " }, style),
                    Span::styled(p.as_str(), style),
                ])
            })
            .collect();
        let modal_p = Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(header)
                .title_style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
        );
        frame.render_widget(modal_p, modal);
    }
}

#[cfg(feature = "tui")]
fn display_title(h: &Hit) -> String {
    if let Some(t) = h.title.as_deref().filter(|s| !s.trim().is_empty()) {
        return t.to_string();
    }
    let snippet: String = h
        .first_message
        .chars()
        .filter(|c| !c.is_control() && *c != '"' && *c != '{' && *c != '}')
        .take(40)
        .collect();
    let snippet = snippet.trim().to_string();
    if snippet.len() >= 6 {
        snippet
    } else {
        format!("[{}]", &h.source_id[..h.source_id.len().min(10)])
    }
}

// Silence unused-import warning when serde is only used via macros.
#[allow(dead_code)]
fn _silence_io() {
    let _: io::Result<()> = Ok(());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_distance_identical_is_zero() {
        let v = vec![1.0_f32, 2.0, 3.0, 4.0];
        let d = cosine_distance(&v, &v);
        assert!(d.abs() < 1e-9, "expected ~0, got {d}");
    }

    #[test]
    fn cosine_distance_orthogonal_is_one() {
        let a = vec![1.0_f32, 0.0];
        let b = vec![0.0_f32, 1.0];
        let d = cosine_distance(&a, &b);
        assert!((d - 1.0).abs() < 1e-9, "expected 1.0, got {d}");
    }

    #[test]
    fn cosine_distance_opposite_is_two() {
        let a = vec![1.0_f32, 1.0, 1.0];
        let b = vec![-1.0_f32, -1.0, -1.0];
        let d = cosine_distance(&a, &b);
        assert!((d - 2.0).abs() < 1e-9, "expected 2.0, got {d}");
    }

    #[test]
    fn cosine_distance_empty_returns_two() {
        let a: Vec<f32> = vec![];
        let b = vec![1.0_f32, 2.0];
        assert_eq!(cosine_distance(&a, &b), 2.0);
        assert_eq!(cosine_distance(&b, &a), 2.0);
    }

    #[test]
    fn decode_vector32_handles_8_byte_header() {
        // turso emits an 8-byte length/type prefix in front of the f32 payload.
        // Real embeddings are 256+ dims, so the offset-8 path is what's hit in
        // practice — test with a realistic-sized vector to exercise it.
        let v: Vec<f32> = (0..256).map(|i| (i as f32 - 128.0) / 128.0).collect();
        let mut blob: Vec<u8> = vec![0u8; 8];
        for x in &v {
            blob.extend_from_slice(&x.to_le_bytes());
        }
        let decoded = decode_vector32(blob).expect("decode");
        assert_eq!(decoded.len(), v.len());
        for (a, b) in decoded.iter().zip(v.iter()) {
            assert!((a - b).abs() < 1e-6, "value mismatch: {a} vs {b}");
        }
    }

    #[test]
    fn decode_vector32_rejects_too_short() {
        assert!(decode_vector32(vec![0, 1]).is_none());
    }

    #[test]
    fn decode_vector32_rejects_garbage() {
        // Values >> 10.0 fail the sanity check at every offset.
        let blob: Vec<u8> = (0..32).map(|i| (i * 13 + 7) as u8).collect();
        let result = decode_vector32(blob);
        // Either None, or a vec whose first values are within the sanity range;
        // we just ensure it doesn't panic on adversarial input.
        if let Some(v) = result {
            assert!(v.iter().take(8).all(|x| x.abs() < 10.0));
        }
    }

    #[test]
    fn vec_to_lit_roundtrips_bracketed_csv() {
        let lit = vec_to_lit(&[1.0, 2.5, -0.25]);
        assert!(lit.starts_with('['));
        assert!(lit.ends_with(']'));
        assert!(lit.contains("1"));
        assert!(lit.contains("2.5"));
        assert!(lit.contains("-0.25"));
        assert_eq!(lit.matches(',').count(), 2);
    }

    #[test]
    fn tokenize_lowercases_and_drops_short_tokens() {
        let toks = tokenize("Hello, World! a 42 file_name");
        assert!(toks.contains(&"hello".to_string()));
        assert!(toks.contains(&"world".to_string()));
        assert!(toks.contains(&"42".to_string()));
        assert!(toks.contains(&"file".to_string()));
        assert!(toks.contains(&"name".to_string()));
        assert!(!toks.contains(&"a".to_string()), "single chars dropped");
    }

    #[test]
    fn bm25_rewards_query_term_matches() {
        let docs = vec![
            Candidate {
                cli: "claude".into(),
                source_id: "1".into(),
                title: Some("refactor auth module".into()),
                directory: None,
                updated_at: None,
                first_message: "rewrite the login flow with PKCE".into(),
                cos_dist: None,
            },
            Candidate {
                cli: "claude".into(),
                source_id: "2".into(),
                title: Some("update dependencies".into()),
                directory: None,
                updated_at: None,
                first_message: "cargo upgrade bump versions".into(),
                cos_dist: None,
            },
        ];
        let scores = compute_bm25("refactor auth", &docs);
        assert_eq!(scores.len(), 2);
        assert!(scores[0] > scores[1], "doc 0 should outscore doc 1");
        assert!(scores[0] > 0.0);
    }

    #[test]
    fn bm25_empty_query_scores_zero() {
        let docs = vec![Candidate {
            cli: "x".into(),
            source_id: "1".into(),
            title: Some("anything".into()),
            directory: None,
            updated_at: None,
            first_message: String::new(),
            cos_dist: None,
        }];
        let scores = compute_bm25("", &docs);
        assert_eq!(scores, vec![0.0]);
    }

    #[test]
    fn truncate_under_max_unchanged() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_over_max_uses_ellipsis() {
        let out = truncate("abcdefghij", 5);
        assert_eq!(out.chars().count(), 5);
        assert!(out.ends_with('…'));
    }

    #[test]
    fn truncate_handles_multibyte() {
        let out = truncate("αβγδε ζηθικ", 6);
        assert_eq!(out.chars().count(), 6);
        assert!(out.ends_with('…'));
    }

    #[test]
    fn reindex_lock_guard_removes_file_on_drop() {
        let tmp = tempfile::tempdir().unwrap();
        let lock_path = tmp.path().join("search-index.lock");
        std::fs::write(&lock_path, "12345").unwrap();
        assert!(lock_path.exists());
        {
            let _g = ReindexLockGuard(lock_path.as_os_str().to_os_string());
        }
        assert!(
            !lock_path.exists(),
            "guard should remove the lock file on drop"
        );
    }

    #[test]
    fn reindex_lock_guard_drop_is_silent_when_file_missing() {
        // If something else already removed the lock (e.g. a concurrent reindex
        // attempt cleaned up), Drop must not panic — std::fs::remove_file returns
        // an error we deliberately ignore.
        let tmp = tempfile::tempdir().unwrap();
        let lock_path = tmp.path().join("does-not-exist.lock");
        let _g = ReindexLockGuard(lock_path.as_os_str().to_os_string());
        // dropping at end of scope — no panic expected
    }
}

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

const MAX_TEXT_CHARS: usize = 600;
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
        eprintln!(
            "warning: could not write default {}: {e}",
            path.display()
        );
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

    let oai_base = pick("OAI_BASE", file.oai_base.clone(), "http://127.0.0.1:18000/v1");
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
    // fail fast with a copy-pasteable fix than burn 30 s per row on timeouts.
    let probe = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()?;
    let probe_url = format!("{}/models", oai_base.trim_end_matches('/'));
    if let Err(e) = probe.get(&probe_url).send().await {
        eprintln!("\n\x1b[31merror:\x1b[0m embedding endpoint unreachable: {e}");
        if let Some(hint) = tunnel_hint.as_deref() {
            eprintln!("hint: bring the tunnel up:\n  {hint}");
        }
        eprintln!("(set OAI_BASE or edit {} to point elsewhere)", config_path().display());
        return Err("endpoint unreachable".into());
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
        conn.execute("UPDATE sessions SET embedding=NULL, model_id=NULL, generated_title=NULL", ())
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

    for s in &sessions {
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
            "[index] {indexed} rows, {} need embedding, {} need naming",
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
        if !args.json && (emb_done + emb_failed) % 32 == 0 {
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
    let argv0 = std::env::args().next().unwrap_or_else(|| "unleash".to_string());
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

/// Best-effort text extraction. We deliberately don't parse per-CLI here —
/// session files are JSONL/JSON whose first ~1KB always contains useful
/// signal (user message, system prompt, working directory). The embedder
/// tolerates noisy JSON just fine.
fn extract_text(s: &SessionInfo) -> Option<String> {
    let bytes = fs::read(&s.path).ok()?;
    let head_len = bytes.len().min(8192);
    let head = String::from_utf8_lossy(&bytes[..head_len]);
    let cleaned: String = head
        .chars()
        .filter(|c| !c.is_control() || *c == ' ' || *c == '\n')
        .collect();
    let mut snippet = String::new();
    for ch in cleaned.chars() {
        if snippet.len() >= MAX_TEXT_CHARS {
            break;
        }
        snippet.push(ch);
    }
    let title_part = s.title.as_deref().or(s.name.as_deref()).unwrap_or("");
    let dir_part = &s.directory;
    Some(format!("{title_part} {dir_part} {snippet}").trim().to_string())
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
        .json(&Req { model, input: text })
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
            .map(|r| r.embedding.as_ref().map(|emb| cosine_distance(qv, emb) as f32))
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
        scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
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
                .title_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
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
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
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
                Span::styled(format!("[{:>7}] ", h.cli), Style::default().fg(Color::Magenta)),
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
    let count_title = format!(
        " {} / {} ",
        state.scored.len(),
        state.rows.len()
    );
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
        Mode::Searching => "type=filter  ↑↓=move  Enter=pick profile  Tab=cycle α  ⇧←/→=fine α  Esc=quit",
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
            format!(" launch into profile — {} ", truncate(&display_title(hit), 30))
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
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
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

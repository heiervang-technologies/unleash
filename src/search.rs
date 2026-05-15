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
const EMBED_TIMEOUT_SECS: u64 = 10;
const MAX_NAMES_PER_RUN: usize = 12;

pub struct RunArgs {
    pub query: Option<String>,
    pub reindex: bool,
    pub json: bool,
    pub top: usize,
}

#[derive(Debug, serde::Serialize)]
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
    let oai_base = env::var("OAI_BASE").unwrap_or_else(|_| "http://localhost:8080/v1".to_string());
    let embed_model =
        env::var("OAI_EMBED_MODEL").unwrap_or_else(|_| "granite-embedding".to_string());
    let chat_base = env::var("OAI_CHAT_BASE").ok().unwrap_or_else(|| oai_base.clone());
    let chat_model = env::var("OAI_CHAT_MODEL").ok();
    let alpha: f32 = env::var("ALPHA")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.4);

    if !args.json {
        eprintln!("unleash search");
        eprintln!("  embeddings: {oai_base} model={embed_model}");
        if let Some(ref m) = chat_model {
            eprintln!("  naming    : {chat_base} model={m}");
        } else {
            eprintln!("  naming    : disabled (set OAI_CHAT_MODEL to enable)");
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

    let t_q = Instant::now();
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
    struct Req<'a> {
        model: &'a str,
        messages: Vec<Msg<'a>>,
        max_tokens: u32,
        temperature: f32,
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

// Silence unused-import warning when serde is only used via macros.
#[allow(dead_code)]
fn _silence_io() {
    let _: io::Result<()> = Ok(());
}

//! PoC: end-to-end semantic session search.
//!
//! Proves the chain: discover → extract text → embed (OAI-compat) →
//! store (turso) → name (OAI-compat chat) → query (vector + BM25 hybrid in Rust).
//!
//! ## How to run
//!
//! 1. Spin up an OpenAI-compatible server with an embedding model. Examples:
//!
//!    # llama.cpp with Granite Embedding small-r2 (preferred per plan):
//!    llama-server -m ~/models/granite-embedding-small-en-r2.Q8_0.gguf --embedding --port 8080 -ngl 99
//!
//!    # Or EmbeddingGemma-300m (fallback if Granite GGUF is fiddly):
//!    llama-server -m ~/models/embeddinggemma-300m.Q8_0.gguf --embedding --port 8080 -ngl 99
//!
//!    # Or via ollama:
//!    ollama serve  # then `ollama pull granite-embedding:30m`
//!
//! 2. Optionally spin up a *chat* model on a second port for session naming
//!    (any small instruct model — qwen2.5-3b, llama3.2-3b, etc.).
//!    If you skip this, naming is disabled and the picker shows raw IDs.
//!
//! 3. Run the PoC:
//!
//!    # Index + query in one shot:
//!    cargo run --release --example poc_search -- "refactor the auth flow"
//!
//!    # Customize endpoints:
//!    OAI_BASE=http://localhost:8080/v1\
//!    OAI_EMBED_MODEL=granite-embedding\
//!    OAI_CHAT_BASE=http://localhost:8081/v1\
//!    OAI_CHAT_MODEL=qwen2.5-3b-instruct\
//!    cargo run --release --example poc_search -- "redo the login flow"
//!
//! The PoC is intentionally fault-tolerant: per-session failures (file read
//! errors, API timeouts, malformed embeddings) are logged and skipped, never
//! abort the whole run.

use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::fs;
use std::path::Path;
use std::time::Instant;

use unleash::interchange::sessions::{discover_all, SessionInfo};

const MAX_TEXT_CHARS: usize = 600; // ~150 tokens; deliberately conservative for llama-server's GPU
const EMBED_BATCH: usize = 1; // sequential — embeddinggemma server CUDA-crashes under parallelism
const TOP_K: usize = 15;
const ALPHA_DEFAULT: f32 = 0.4; // 0=pure semantic, 1=pure BM25
const NAMING_TIMEOUT_SECS: u64 = 60; // CPU naming via qwen2.5-3b is slow but reliable
const EMBED_TIMEOUT_SECS: u64 = 10;
/// Cap naming pass at the N most-recent unnamed sessions per run. PoC default;
/// production code (`unleash sessions name`) will background-process all of them.
const MAX_NAMES_PER_RUN: usize = 12;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn Error>> {
    let query = env::args().nth(1).unwrap_or_else(|| "test".to_string());

    let oai_base = env::var("OAI_BASE").unwrap_or_else(|_| "http://localhost:8080/v1".to_string());
    let embed_model =
        env::var("OAI_EMBED_MODEL").unwrap_or_else(|_| "granite-embedding".to_string());
    let chat_base = env::var("OAI_CHAT_BASE")
        .ok()
        .unwrap_or_else(|| oai_base.clone());
    let chat_model = env::var("OAI_CHAT_MODEL").ok();
    let alpha: f32 = env::var("ALPHA")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(ALPHA_DEFAULT);

    eprintln!("poc_search: query = {query:?}");
    eprintln!("  embeddings: {oai_base} model={embed_model}");
    if let Some(ref m) = chat_model {
        eprintln!("  naming    : {chat_base} model={m}");
    } else {
        eprintln!("  naming    : disabled (set OAI_CHAT_MODEL to enable)");
    }
    eprintln!("  alpha     : {alpha:.2}  (0=semantic, 1=bm25)");

    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(EMBED_TIMEOUT_SECS))
        .build()?;

    // ── Step 1: open turso DB and bootstrap schema ────────────────────────
    let db_path = dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join("unleash/search-poc.db");
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent)?;
    }
    eprintln!("  store     : {}", db_path.display());

    let db = turso::Builder::new_local(db_path.to_string_lossy().as_ref())
        .build()
        .await?;
    let conn = db.connect()?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS sessions (\
            pk INTEGER PRIMARY KEY,\
            cli TEXT NOT NULL,\
            source_id TEXT NOT NULL,\
            path TEXT NOT NULL,\
            directory TEXT,\
            native_title TEXT,\
            generated_title TEXT,\
            first_message TEXT,\
            updated_at TEXT,\
            mtime_ns INTEGER NOT NULL,\
            model_id TEXT,\
            embedding BLOB,\
            UNIQUE(cli, source_id))",
        (),
    )
    .await?;

    // ── Step 2: discover + extract text + upsert if mtime changed ─────────
    let t_disc = Instant::now();
    let sessions = discover_all();
    eprintln!(
        "\n[discover] {} sessions in {:?}",
        sessions.len(),
        t_disc.elapsed()
    );

    let mut indexed_pks: Vec<i64> = Vec::new();
    let mut needs_embed: Vec<(i64, String)> = Vec::new();
    let mut needs_name: Vec<(i64, String)> = Vec::new();

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
                "INSERT INTO sessions (cli, source_id, path, directory, native_title, \
                    first_message, updated_at, mtime_ns, model_id) \
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9) \
                 ON CONFLICT(cli, source_id) DO UPDATE SET \
                    path=excluded.path, directory=excluded.directory,\
                    native_title=excluded.native_title, first_message=excluded.first_message,\
                    updated_at=excluded.updated_at, mtime_ns=excluded.mtime_ns,\
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

        let pk: i64 = {
            let mut r = conn
                .query(
                    "SELECT pk, embedding IS NOT NULL AS has_emb, generated_title IS NOT NULL OR native_title IS NOT NULL AS has_title \
                     FROM sessions WHERE cli=?1 AND source_id=?2",
                    turso::params![s.cli.clone(), s.id.clone()],
                )
                .await?;
            let row = r.next().await?.expect("row must exist after upsert");
            let pk: i64 = row.get(0)?;
            let has_emb: i64 = row.get(1)?;
            let has_title: i64 = row.get(2)?;
            indexed_pks.push(pk);
            if has_emb == 0 && !text.is_empty() {
                needs_embed.push((pk, text.clone()));
            }
            if has_title == 0 && chat_model.is_some() && !text.is_empty() {
                needs_name.push((pk, text.clone()));
            }
            pk
        };
        let _ = pk;
    }
    eprintln!(
        "[index] {} rows indexed, {} need embedding, {} need naming",
        indexed_pks.len(),
        needs_embed.len(),
        needs_name.len()
    );

    // ── Step 3: embed missing rows in batches ─────────────────────────────
    let t_emb = Instant::now();
    let mut embed_dim: Option<usize> = None;
    let total = needs_embed.len();
    let mut done = 0usize;
    let mut failed = 0usize;
    for chunk in needs_embed.chunks(EMBED_BATCH) {
        let texts: Vec<String> = chunk.iter().map(|(_, t)| t.clone()).collect();
        // Try the batch first. On any error, fall back to one-at-a-time so a
        // single oversize input doesn't tank the rest of the chunk (llama-server
        // returns 500 for the whole batch if any item exceeds the batch budget).
        let vecs: Vec<Option<Vec<f32>>> = match embed_batch(&http, &oai_base, &embed_model, &texts)
            .await
        {
            Ok(v) => v.into_iter().map(Some).collect(),
            Err(_) => {
                let mut out = Vec::with_capacity(texts.len());
                for t in &texts {
                    match embed_batch(&http, &oai_base, &embed_model, std::slice::from_ref(t)).await
                    {
                        Ok(mut v) => out.push(v.pop()),
                        Err(_) => out.push(None),
                    }
                }
                out
            }
        };
        for ((pk, _), maybe_v) in chunk.iter().zip(vecs.into_iter()) {
            match maybe_v {
                Some(v) => {
                    embed_dim = Some(v.len());
                    let lit = vec_to_lit(&v);
                    conn.execute(
                        "UPDATE sessions SET embedding = vector32(?1), model_id = ?2 WHERE pk = ?3",
                        turso::params![lit, embed_model.clone(), *pk],
                    )
                    .await?;
                    done += 1;
                }
                None => failed += 1,
            }
        }
        if (done + failed).is_multiple_of(64) {
            eprintln!(
                "  embed progress: {}/{} ({} failed)",
                done + failed,
                total,
                failed
            );
        }
    }
    eprintln!("[embed] {done}/{total} embedded, {failed} failed");
    eprintln!("[embed] dim={:?} elapsed={:?}", embed_dim, t_emb.elapsed());

    // ── Step 4: name missing rows ─────────────────────────────────────────
    if let Some(ref cm) = chat_model {
        let t_name = Instant::now();
        let name_http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(NAMING_TIMEOUT_SECS))
            .build()?;
        // Sessions are already sorted by recency (discover_all does this) so the
        // tail of needs_name is the recent stuff. Take from the front (= newest).
        let to_name: Vec<&(i64, String)> = needs_name.iter().take(MAX_NAMES_PER_RUN).collect();
        eprintln!(
            "[name] naming {}/{} most-recent unnamed sessions",
            to_name.len(),
            needs_name.len()
        );
        let mut named = 0usize;
        for (pk, text) in &to_name {
            match generate_name(&name_http, &chat_base, cm, text).await {
                Ok(name) => {
                    eprintln!("  pk={pk} -> {name:?}");
                    conn.execute(
                        "UPDATE sessions SET generated_title = ?1 WHERE pk = ?2",
                        turso::params![name.clone(), *pk],
                    )
                    .await?;
                    named += 1;
                }
                Err(e) => eprintln!("  name pk={pk} failed: {e}"),
            }
        }
        eprintln!("[name] {named} named in {:?}", t_name.elapsed());
    }

    // ── Step 5: run hybrid search ────────────────────────────────────────
    let t_q = Instant::now();
    let qvec = embed_batch(&http, &oai_base, &embed_model, std::slice::from_ref(&query)).await?;
    let qvec = qvec.into_iter().next().ok_or("empty query embedding")?;
    let qlit = vec_to_lit(&qvec);

    // Pull all candidates + their cosine distance
    let mut candidates: Vec<Candidate> = Vec::new();
    let mut rows = conn
        .query(
            "SELECT pk, cli, source_id, coalesce(generated_title, native_title) AS title, \
                    directory, updated_at, first_message, \
                    CASE WHEN embedding IS NOT NULL \
                         THEN vector_distance_cos(embedding, vector32(?1)) ELSE NULL END AS cos \
             FROM sessions",
            turso::params![qlit],
        )
        .await?;
    while let Some(row) = rows.next().await? {
        let pk: i64 = row.get(0)?;
        let cli: String = row.get(1)?;
        let source_id: String = row.get(2)?;
        let title: Option<String> = row.get(3).ok();
        let directory: Option<String> = row.get(4).ok();
        let updated_at: Option<String> = row.get(5).ok();
        let first_message: Option<String> = row.get(6).ok();
        let cos: Option<f64> = row.get(7).ok();
        candidates.push(Candidate {
            pk,
            cli,
            source_id,
            title,
            directory,
            updated_at,
            first_message: first_message.unwrap_or_default(),
            cos_dist: cos,
            bm25: 0.0,
        });
    }

    // BM25 in Rust over the corpus
    let bm25 = compute_bm25(&query, &candidates);
    for (c, score) in candidates.iter_mut().zip(bm25.iter()) {
        c.bm25 = *score;
    }

    // Blend
    let max_bm25 = candidates
        .iter()
        .map(|c| c.bm25)
        .fold(0.0f32, f32::max)
        .max(1e-6);
    let mut scored: Vec<(f32, &Candidate)> = candidates
        .iter()
        .map(|c| {
            let bm_norm = c.bm25 / max_bm25;
            let sem_norm = c.cos_dist.map(|d| 1.0 - (d as f32) / 2.0).unwrap_or(0.0);
            let score = alpha * bm_norm + (1.0 - alpha) * sem_norm;
            (score, c)
        })
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    println!("\n=== top {TOP_K} results (alpha={alpha:.2}) ===");
    for (i, (score, c)) in scored.iter().take(TOP_K).enumerate() {
        // Display fallback chain: generated/native title → first-message snippet → id prefix
        let snippet_fallback: String = c
            .first_message
            .chars()
            .filter(|ch| !ch.is_control() && *ch != '"' && *ch != '{' && *ch != '}')
            .take(40)
            .collect::<String>()
            .trim()
            .to_string();
        let title_owned: String = c
            .title
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                if snippet_fallback.len() >= 6 {
                    snippet_fallback
                } else {
                    format!("[{}]", &c.source_id[..c.source_id.len().min(10)])
                }
            });
        let title = title_owned.as_str();
        let dir = c.directory.as_deref().unwrap_or("");
        let date = c
            .updated_at
            .as_deref()
            .unwrap_or("")
            .chars()
            .take(10)
            .collect::<String>();
        let cos = c
            .cos_dist
            .map(|d| format!("cos={:.3}", d))
            .unwrap_or_else(|| "cos=∅".to_string());
        println!(
            "{:>2}. score={:.3}  bm25={:.3}  {}  [{:>8}]  {:<32}  {:<24}  {}",
            i + 1,
            score,
            c.bm25,
            cos,
            c.cli,
            truncate(title, 32),
            truncate(dir, 24),
            date
        );
    }
    eprintln!(
        "\n[query] hybrid scored {} rows in {:?}",
        candidates.len(),
        t_q.elapsed()
    );
    Ok(())
}

// ── helpers ─────────────────────────────────────────────────────────────

#[derive(Debug)]
struct Candidate {
    #[allow(dead_code)]
    pk: i64,
    cli: String,
    source_id: String,
    title: Option<String>,
    directory: Option<String>,
    updated_at: Option<String>,
    first_message: String,
    cos_dist: Option<f64>,
    bm25: f32,
}

#[allow(dead_code)]
impl Candidate {
    fn handle(&self) -> String {
        format!("{}:{}", self.cli, self.source_id)
    }
}

fn file_mtime_ns(p: &Path) -> Option<i64> {
    let md = fs::metadata(p).ok()?;
    let mtime = md.modified().ok()?;
    let dur = mtime.duration_since(std::time::UNIX_EPOCH).ok()?;
    Some(dur.as_nanos() as i64)
}

/// Best-effort: pull a useful text snippet from the session file.
/// We don't parse per-CLI here; just grab the first ~1KB of decoded UTF-8
/// from the file, strip control chars, take the first MAX_TEXT_CHARS.
/// Good enough for the PoC — embeddings tolerate noisy text.
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
    Some(
        format!("{title_part} {dir_part} {snippet}")
            .trim()
            .to_string(),
    )
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

async fn embed_batch(
    http: &reqwest::Client,
    base: &str,
    model: &str,
    texts: &[String],
) -> Result<Vec<Vec<f32>>, Box<dyn Error>> {
    #[derive(serde::Serialize)]
    struct Req<'a> {
        model: &'a str,
        input: &'a [String],
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
            input: texts,
        })
        .send()
        .await?
        .error_for_status()?
        .json::<Resp>()
        .await?;
    Ok(resp.data.into_iter().map(|i| i.embedding).collect())
}

async fn generate_name(
    http: &reqwest::Client,
    base: &str,
    model: &str,
    text: &str,
) -> Result<String, Box<dyn Error>> {
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

/// Standard BM25 with k1=1.5, b=0.75 over the candidate corpus.
/// Index built from candidate `title + first_message + directory + cli`.
fn compute_bm25(query: &str, docs: &[Candidate]) -> Vec<f32> {
    let q_terms: Vec<String> = tokenize(query);
    if q_terms.is_empty() || docs.is_empty() {
        return vec![0.0; docs.len()];
    }

    // Build tokenized docs
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

    // df per query term
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

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

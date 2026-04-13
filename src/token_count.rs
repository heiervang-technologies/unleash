//! Token counting for conversation transcripts.
//!
//! Supports two backends:
//! - `tiktoken` (cl100k_base) — for Claude, GPT-4, etc.
//! - `huggingface` — for local models with a tokenizer.json

use std::fs;
use std::io::{self, BufRead};
use std::path::Path;

/// Which tokenizer backend to use.
pub enum Backend {
    /// OpenAI cl100k_base via tiktoken-rs (default for Claude/GPT).
    Tiktoken,
    /// HuggingFace tokenizers — requires path to tokenizer.json.
    HuggingFace(String),
}

impl Backend {
    pub fn from_args(tokenizer_path: Option<&str>) -> Self {
        match tokenizer_path {
            Some(p) => Backend::HuggingFace(p.to_string()),
            None => Backend::Tiktoken,
        }
    }
}

/// Count tokens in a file. Reads line-by-line to stay memory-efficient.
pub fn count_file(path: &Path, backend: &Backend) -> io::Result<usize> {
    match backend {
        Backend::Tiktoken => count_file_tiktoken(path),
        Backend::HuggingFace(tokenizer_path) => count_file_hf(path, tokenizer_path),
    }
}

fn count_file_tiktoken(path: &Path) -> io::Result<usize> {
    let bpe = tiktoken_rs::cl100k_base()
        .map_err(|e| io::Error::other(format!("Failed to load cl100k_base: {e}")))?;

    let file = fs::File::open(path)?;
    let reader = io::BufReader::new(file);
    let mut total = 0;

    for line in reader.lines() {
        let line = line?;
        if !line.is_empty() {
            total += bpe.encode_with_special_tokens(&line).len();
        }
    }

    Ok(total)
}

fn count_file_hf(path: &Path, tokenizer_path: &str) -> io::Result<usize> {
    let tokenizer = tokenizers::Tokenizer::from_file(tokenizer_path)
        .map_err(|e| io::Error::other(format!("Failed to load tokenizer from {tokenizer_path}: {e}")))?;

    let file = fs::File::open(path)?;
    let reader = io::BufReader::new(file);
    let mut total = 0;

    for line in reader.lines() {
        let line = line?;
        if !line.is_empty() {
            match tokenizer.encode(line.as_str(), false) {
                Ok(encoding) => total += encoding.get_ids().len(),
                Err(e) => {
                    eprintln!("warning: tokenization error on line, skipping: {e}");
                }
            }
        }
    }

    Ok(total)
}

/// Handle the `unleash token-count` subcommand.
pub fn handle_token_count(file: &str, tokenizer: Option<&str>) -> io::Result<()> {
    let path = Path::new(file);
    if !path.exists() {
        eprintln!("error: file not found: {file}");
        return Err(io::Error::other("file not found"));
    }

    let backend = Backend::from_args(tokenizer);
    let count = count_file(path, &backend)?;
    println!("{count}");
    Ok(())
}

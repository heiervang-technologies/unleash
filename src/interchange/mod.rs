pub mod claude;
pub mod codex;
#[cfg(test)]
mod cross_cli_tests;
pub mod gemini;
pub(crate) mod helpers;
pub mod hub;
pub mod inject;
#[cfg(test)]
mod lossless_tests;
pub mod opencode;
pub mod pi;
pub mod semantic_eq;
pub mod sessions;

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliFormat {
    ClaudeCode,
    Codex,
    GeminiCli,
    OpenCode,
    Pi,
    Ucf,
}

impl fmt::Display for CliFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ClaudeCode => write!(f, "claude-code"),
            Self::Codex => write!(f, "codex"),
            Self::GeminiCli => write!(f, "gemini-cli"),
            Self::OpenCode => write!(f, "opencode"),
            Self::Pi => write!(f, "pi"),
            Self::Ucf => write!(f, "ucf"),
        }
    }
}

impl std::str::FromStr for CliFormat {
    type Err = ConvertError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "claude" | "claude-code" => Ok(Self::ClaudeCode),
            "codex" => Ok(Self::Codex),
            "gemini" | "gemini-cli" => Ok(Self::GeminiCli),
            "opencode" => Ok(Self::OpenCode),
            "pi" | "pi-coding-agent" => Ok(Self::Pi),
            "ucf" | "hub" => Ok(Self::Ucf),
            _ => Err(ConvertError::InvalidFormat(format!(
                "Unknown CLI format: {s}"
            ))),
        }
    }
}

#[derive(Debug)]
pub enum ConvertError {
    Io(std::io::Error),
    Json(serde_json::Error),
    Sqlite(rusqlite::Error),
    InvalidFormat(String),
    #[allow(dead_code)]
    UnsupportedVersion(String),
}

impl fmt::Display for ConvertError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "IO error: {e}"),
            Self::Json(e) => write!(f, "JSON error: {e}"),
            Self::Sqlite(e) => write!(f, "SQLite error: {e}"),
            Self::InvalidFormat(msg) => write!(f, "Invalid format: {msg}"),
            Self::UnsupportedVersion(v) => write!(f, "Unsupported UCF version: {v}"),
        }
    }
}

impl std::error::Error for ConvertError {}

impl From<std::io::Error> for ConvertError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<serde_json::Error> for ConvertError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

impl From<rusqlite::Error> for ConvertError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Sqlite(e)
    }
}

/// CLI entry point for `unleash convert`.
pub fn convert_command(
    from: &str,
    to: &str,
    input: &str,
    output: Option<&str>,
    verify: bool,
) -> Result<(), ConvertError> {
    use std::io::{BufReader, Write};

    let input_data = std::fs::read_to_string(input)?;

    // Parse source format to Hub
    let hub_records = match from {
        "claude" | "claude-code" => {
            let reader = BufReader::new(input_data.as_bytes());
            claude::to_hub(reader)?
        }
        "codex" => {
            let reader = BufReader::new(input_data.as_bytes());
            codex::to_hub(reader)?
        }
        "gemini" | "gemini-cli" => gemini::to_hub(input_data.as_bytes())?,
        "pi" | "pi-coding-agent" => {
            let reader = BufReader::new(input_data.as_bytes());
            pi::to_hub(reader)?
        }
        "opencode" => {
            let messages: Vec<serde_json::Value> = serde_json::from_str(&input_data)?;
            // Parts file: same directory, replace -messages.json with -parts.json
            let parts_path = input.replace("-messages.json", "-parts.json");
            let parts_data = std::fs::read_to_string(&parts_path)?;
            let parts: Vec<serde_json::Value> = serde_json::from_str(&parts_data)?;
            let oc_input = opencode::OpenCodeInput {
                session_id: "opencode-session".to_string(),
                messages,
                parts,
            };
            opencode::to_hub(&oc_input)?
        }
        "hub" => {
            // Already hub format — parse directly
            let reader = BufReader::new(input_data.as_bytes());
            let mut records = Vec::new();
            for line in std::io::BufRead::lines(reader) {
                let line = line?;
                if line.trim().is_empty() {
                    continue;
                }
                let record: hub::HubRecord = serde_json::from_str(&line)?;
                records.push(record);
            }
            records
        }
        _ => {
            return Err(ConvertError::InvalidFormat(format!(
                "Unsupported source format: {from}. Supported: claude, codex, gemini, opencode, pi, hub"
            )));
        }
    };

    if verify {
        // Round-trip verify: convert back and compare
        let back = match from {
            "claude" | "claude-code" => claude::from_hub(&hub_records)?,
            "codex" => codex::from_hub(&hub_records)?,
            "pi" | "pi-coding-agent" => pi::from_hub(&hub_records)?,
            "gemini" | "gemini-cli" => {
                let back_val = gemini::from_hub(&hub_records)?;
                // Gemini is a single JSON file, compare the whole object
                let orig_val: serde_json::Value = serde_json::from_str(&input_data)?;
                if let Err(diff) = semantic_eq::semantic_eq(&orig_val, &back_val) {
                    eprintln!("Mismatch: {diff}");
                    return Err(ConvertError::InvalidFormat(
                        "Round-trip verification failed".into(),
                    ));
                }
                println!("Lossless round-trip verified: Gemini session OK");
                return Ok(());
            }
            "opencode" => {
                let back = opencode::from_hub(&hub_records)?;
                // Compare messages
                let orig_msgs: Vec<serde_json::Value> = serde_json::from_str(&input_data)?;
                let mut mismatches = 0;
                for (i, (orig, result)) in orig_msgs.iter().zip(back.messages.iter()).enumerate() {
                    if let Err(diff) = semantic_eq::semantic_eq(orig, result) {
                        eprintln!("Message {i}: {diff}");
                        mismatches += 1;
                    }
                }
                // Compare parts
                let parts_path = input.replace("-messages.json", "-parts.json");
                let parts_data = std::fs::read_to_string(&parts_path)?;
                let orig_parts: Vec<serde_json::Value> = serde_json::from_str(&parts_data)?;
                for (i, (orig, result)) in orig_parts.iter().zip(back.parts.iter()).enumerate() {
                    if let Err(diff) = semantic_eq::semantic_eq(orig, result) {
                        eprintln!("Part {i}: {diff}");
                        mismatches += 1;
                    }
                }
                if mismatches == 0 {
                    println!(
                        "Lossless round-trip verified: {} messages, {} parts OK",
                        orig_msgs.len(),
                        orig_parts.len()
                    );
                } else {
                    eprintln!("{mismatches} mismatches found");
                    return Err(ConvertError::InvalidFormat(
                        "Round-trip verification failed".into(),
                    ));
                }
                return Ok(());
            }
            _ => {
                return Err(ConvertError::InvalidFormat(format!(
                    "Verify not yet supported for format: {from}"
                )));
            }
        };

        // Compare each line (JSONL formats)
        let original_lines: Vec<serde_json::Value> = input_data
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(serde_json::from_str)
            .collect::<Result<Vec<_>, _>>()?;

        let mut mismatches = 0;
        for (i, (orig, result)) in original_lines.iter().zip(back.iter()).enumerate() {
            if let Err(diff) = semantic_eq::semantic_eq(orig, result) {
                eprintln!("Line {i}: {diff}");
                mismatches += 1;
            }
        }

        if original_lines.len() != back.len() {
            eprintln!(
                "Line count mismatch: original={}, result={}",
                original_lines.len(),
                back.len()
            );
            mismatches += 1;
        }

        if mismatches == 0 {
            println!(
                "Lossless round-trip verified: {} lines OK",
                original_lines.len()
            );
        } else {
            eprintln!("{mismatches} mismatches found");
            return Err(ConvertError::InvalidFormat(
                "Round-trip verification failed".into(),
            ));
        }

        return Ok(());
    }

    // Convert to target format
    let output_lines: Vec<String> = if to == "hub" {
        hub_records
            .iter()
            .map(|r| serde_json::to_string(r).map_err(ConvertError::from))
            .collect::<Result<Vec<_>, _>>()?
    } else {
        let values = match to {
            "claude" | "claude-code" => claude::from_hub(&hub_records)?,
            "codex" => codex::from_hub(&hub_records)?,
            "pi" | "pi-coding-agent" => pi::from_hub(&hub_records)?,
            "gemini" | "gemini-cli" => {
                let val = gemini::from_hub(&hub_records)?;
                let json = serde_json::to_string_pretty(&val)?;
                match output {
                    Some(path) => {
                        let mut f = std::fs::File::create(path)?;
                        std::io::Write::write_all(&mut f, json.as_bytes())?;
                        eprintln!("Wrote Gemini session to {path}");
                    }
                    None => print!("{json}"),
                }
                return Ok(());
            }
            "opencode" => {
                let oc_output = opencode::from_hub(&hub_records)?;
                let msgs_json = serde_json::to_string_pretty(&oc_output.messages)?;
                let parts_json = serde_json::to_string_pretty(&oc_output.parts)?;
                match output {
                    Some(path) => {
                        let mut f = std::fs::File::create(path)?;
                        std::io::Write::write_all(&mut f, msgs_json.as_bytes())?;
                        eprintln!("Wrote OpenCode messages to {path}");
                        let parts_path = path.replace("-messages.json", "-parts.json");
                        let mut pf = std::fs::File::create(&parts_path)?;
                        std::io::Write::write_all(&mut pf, parts_json.as_bytes())?;
                        eprintln!("Wrote OpenCode parts to {parts_path}");
                    }
                    None => {
                        println!("=== MESSAGES ===");
                        print!("{msgs_json}");
                        println!("\n=== PARTS ===");
                        print!("{parts_json}");
                    }
                }
                return Ok(());
            }
            _ => {
                return Err(ConvertError::InvalidFormat(format!(
                    "Unsupported target format: {to}. Supported: claude, codex, gemini, opencode, pi, hub"
                )));
            }
        };
        values
            .iter()
            .map(|v| serde_json::to_string(v).map_err(ConvertError::from))
            .collect::<Result<Vec<_>, _>>()?
    };

    // Write output
    let output_str = output_lines.join("\n") + "\n";
    match output {
        Some(path) => {
            let mut f = std::fs::File::create(path)?;
            f.write_all(output_str.as_bytes())?;
            eprintln!("Wrote {} lines to {path}", output_lines.len());
        }
        None => {
            print!("{output_str}");
        }
    }

    Ok(())
}

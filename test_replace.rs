fn replace_context_block(existing: &str, name: &str, replacement: &str) -> String {
    let start = format!("<!-- unleash-skillsync-start: {name} -->");
    let end = format!("<!-- unleash-skillsync-end: {name} -->");

    let mut cleaned = existing.to_string();
    while let Some(start_idx) = cleaned.find(&start) {
        if let Some(rel_end_idx) = cleaned[start_idx..].find(&end) {
            let end_idx = start_idx + rel_end_idx + end.len();
            let before = cleaned[..start_idx].trim_end();
            let after = cleaned[end_idx..].trim_start();
            if before.is_empty() && after.is_empty() {
                cleaned = String::new();
            } else if before.is_empty() {
                cleaned = after.to_string();
            } else if after.is_empty() {
                cleaned = before.to_string();
            } else {
                cleaned = format!("{}\n\n{}", before, after);
            }
        } else {
            break;
        }
    }

    let mut out = cleaned.trim_end().to_string();
    if !out.is_empty() && !replacement.trim().is_empty() {
        out.push_str("\n\n");
    }
    out.push_str(replacement.trim_end());
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

fn main() {
    let existing = "some text\n<!-- unleash-skillsync-start: test -->\nold block\n<!-- unleash-skillsync-end: test -->\nmore text\n<!-- unleash-skillsync-start: test -->\nduplicate block\n<!-- unleash-skillsync-end: test -->\nend text";
    let replacement = "<!-- unleash-skillsync-start: test -->\nnew block\n<!-- unleash-skillsync-end: test -->";
    println!("{}", replace_context_block(existing, "test", replacement));
}

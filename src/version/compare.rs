/// Canonically compare two version strings (semver-like).
///
/// - Strips known prefixes ("v", "rust-v") from both inputs.
/// - Pre-release versions (with `-` suffix) are less than the same base version.
/// - Splits on `.` and compares each numeric segment without integer overflow.
/// - Zero-pads shorter versions so "1.2" == "1.2.0".
pub(crate) fn version_compare(a: &str, b: &str) -> std::cmp::Ordering {
    fn strip_prefix(s: &str) -> &str {
        s.trim_start_matches("rust-v").trim_start_matches('v')
    }

    /// Split a version string into (base numeric parts, optional pre-release suffix).
    fn parse_parts(s: &str) -> (Vec<&str>, Option<&str>) {
        let pre = s.split_once('-').map(|(_, rest)| rest);
        let base = s.split('-').next().unwrap_or(s);
        let parts = base.split('.').collect();
        (parts, pre)
    }

    let a_stripped = strip_prefix(a);
    let b_stripped = strip_prefix(b);
    let (a_parts, a_pre) = parse_parts(a_stripped);
    let (b_parts, b_pre) = parse_parts(b_stripped);

    // Compare base numeric parts
    for i in 0..a_parts.len().max(b_parts.len()) {
        let pa = a_parts.get(i).copied().unwrap_or("0");
        let pb = b_parts.get(i).copied().unwrap_or("0");
        match compare_numeric_segment(pa, pb) {
            std::cmp::Ordering::Equal => continue,
            other => return other,
        }
    }

    // Same base version: pre-release < release (per semver)
    match (a_pre, b_pre) {
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (Some(a_suffix), Some(b_suffix)) => compare_prerelease(a_suffix, b_suffix),
        (None, None) => std::cmp::Ordering::Equal,
    }
}

fn compare_numeric_segment(a: &str, b: &str) -> std::cmp::Ordering {
    fn normalize(s: &str) -> &str {
        if s.chars().all(|c| c.is_ascii_digit()) {
            let stripped = s.trim_start_matches('0');
            if stripped.is_empty() {
                "0"
            } else {
                stripped
            }
        } else {
            "0"
        }
    }

    let a = normalize(a);
    let b = normalize(b);
    match a.len().cmp(&b.len()) {
        std::cmp::Ordering::Equal => a.cmp(b),
        other => other,
    }
}

/// Compare pre-release suffixes per SemVer 11.4:
/// split on `.`, numeric segments compare as integers, otherwise lexicographic.
fn compare_prerelease(a: &str, b: &str) -> std::cmp::Ordering {
    let a_parts: Vec<&str> = a.split('.').collect();
    let b_parts: Vec<&str> = b.split('.').collect();

    for i in 0..a_parts.len().max(b_parts.len()) {
        let ap = a_parts.get(i);
        let bp = b_parts.get(i);
        match (ap, bp) {
            (None, Some(_)) => return std::cmp::Ordering::Less, // fewer fields = lower precedence
            (Some(_), None) => return std::cmp::Ordering::Greater,
            (Some(a_seg), Some(b_seg)) => {
                let ord = match (
                    a_seg.chars().all(|c| c.is_ascii_digit()),
                    b_seg.chars().all(|c| c.is_ascii_digit()),
                ) {
                    (true, true) => compare_numeric_segment(a_seg, b_seg),
                    (true, false) => std::cmp::Ordering::Less, // numeric < alphanumeric
                    (false, true) => std::cmp::Ordering::Greater,
                    (false, false) => a_seg.cmp(b_seg),
                };
                if ord != std::cmp::Ordering::Equal {
                    return ord;
                }
            }
            (None, None) => break,
        }
    }
    std::cmp::Ordering::Equal
}

/// Convenience wrapper: returns `true` if version `a` is strictly less than `b`.
pub(crate) fn version_less_than(a: &str, b: &str) -> bool {
    version_compare(a, b) == std::cmp::Ordering::Less
}

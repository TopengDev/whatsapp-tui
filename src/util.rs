/// Truncate a string to at most `max_chars` characters, appending "..." if truncated.
/// Safe for multi-byte UTF-8 (emoji, CJK, etc.) — never slices mid-codepoint.
pub fn truncate(s: &str, max_chars: usize) -> String {
    let mut end = 0;
    let mut count = 0;
    for (i, _) in s.char_indices() {
        if count >= max_chars {
            return format!("{}...", &s[..end]);
        }
        end = i;
        count += 1;
    }
    // Didn't exceed — check if the last char pushes us over
    if count >= max_chars && s.len() > end {
        format!("{}...", &s[..end])
    } else {
        s.to_string()
    }
}

/// Expand `~/` to the home directory.
pub fn shellexpand(path: &str) -> String {
    if path.starts_with("~/") {
        if let Some(home) = dirs::home_dir() {
            return format!("{}{}", home.display(), &path[1..]);
        }
    }
    path.to_string()
}

/// Truncate a string to at most `max_chars` characters, no ellipsis.
pub fn truncate_bare(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        Some((i, _)) => &s[..i],
        None => s,
    }
}

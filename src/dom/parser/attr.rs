/// Extract the value of `key` from a raw HTML attribute string.
///
/// Handles both quoted (`key="val"`, `key='val'`) and unquoted (`key=val`) forms.
pub fn get_attr<'a>(attrs: &'a str, key: &str) -> Option<&'a str> {
    let lower = attrs.to_ascii_lowercase();
    
    // 1. Try key="..." form
    let needle = format!("{}=", key);
    if let Some(pos) = lower.find(needle.as_str()) {
        let start = pos + needle.len();
        let rest = &attrs[start..];
        if rest.is_empty() { return Some(""); } 
        let quote = if rest.starts_with('"') || rest.starts_with('\'') {
            Some(rest.as_bytes()[0] as char)
        } else {
            None
        };
        let val_start = if quote.is_some() { &rest[1..] } else { rest };
        let end = if let Some(q) = quote {
            val_start.find(q).unwrap_or(val_start.len())
        } else {
            val_start.find(|c: char| c.is_ascii_whitespace() || c == '>')
                     .unwrap_or(val_start.len())
        };
        return Some(&val_start[..end]);
    }
    
    // 2. Try boolean form (just key)
    // Check if key is present as a whole word
    for (i, _) in lower.match_indices(key) {
        // word must be preceded by whitespace or start of string
        let prev_ok = i == 0 || lower.as_bytes()[i-1].is_ascii_whitespace();
        if !prev_ok { continue; }
        
        let end = i + key.len();
        // word must be followed by whitespace, end of string, or '>'
        let next_ok = end == lower.len() 
            || lower.as_bytes()[end].is_ascii_whitespace() 
            || lower.as_bytes()[end] == b'>';
        
        if next_ok {
            return Some(""); // Return empty string to indicate presence
        }
    }

    None
}

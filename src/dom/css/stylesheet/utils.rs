/// Skip whitespace characters in `chars` starting at `pos`.
pub fn skip_whitespace(chars: &[char], pos: &mut usize) {
    while *pos < chars.len() && chars[*pos].is_whitespace() {
        *pos += 1;
    }
}

/// Skip an at-rule starting at `@`.
pub fn skip_at_rule(chars: &[char], pos: &mut usize) {
    *pos += 1; // skip `@`
    while *pos < chars.len() {
        match chars[*pos] {
            ';' => { *pos += 1; return; }
            '{' => { *pos += 1; skip_to_closing_brace(chars, pos); return; }
            _   => { *pos += 1; }
        }
    }
}

/// Advance `pos` past the next `}`.
pub fn skip_to_closing_brace(chars: &[char], pos: &mut usize) {
    let mut depth = 1usize;
    while *pos < chars.len() {
        match chars[*pos] {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 { *pos += 1; return; }
            }
            _ => {}
        }
        *pos += 1;
    }
}

/// Collect identifier characters (alphanumeric, `-`, `_`) from `chars` starting at `*pos`.
pub fn take_ident(chars: &[char], pos: &mut usize) -> String {
    let mut s = String::new();
    while *pos < chars.len() {
        let c = chars[*pos];
        if c.is_alphanumeric() || c == '-' || c == '_' {
            s.push(c);
            *pos += 1;
        } else {
            break;
        }
    }
    s
}

/// Like `take_ident`, but also accepts hyphen (same as `take_ident` — kept for clarity).
pub fn take_ident_hyphenated(chars: &[char], pos: &mut usize) -> String {
    take_ident(chars, pos)
}

/// Collect all characters up to (but not including) `stop`, advancing `pos`.
pub fn take_until(chars: &[char], pos: &mut usize, stop: char) -> String {
    let mut s = String::new();
    while *pos < chars.len() && chars[*pos] != stop {
        s.push(chars[*pos]);
        *pos += 1;
    }
    s
}

/// Collect the content of a pseudo-class argument `(…)`, handling nested parens.
/// Assumes the opening `(` was already consumed.  Stops when the matching `)`
/// is found.  On return, `*pos` points to the character AFTER the closing `)`.
pub fn take_pseudo_arg(chars: &[char], pos: &mut usize) -> String {
    let mut s = String::new();
    let mut depth = 1usize;
    while *pos < chars.len() && depth > 0 {
        match chars[*pos] {
            '(' => { depth += 1; s.push('('); *pos += 1; }
            ')' => {
                depth -= 1;
                *pos += 1;
                if depth > 0 { s.push(')'); }
            }
            c => { s.push(c); *pos += 1; }
        }
    }
    s
}

/// Remove `/* … */` block comments from the source string.
pub fn strip_comments(src: &str) -> String {
    let mut result = String::with_capacity(src.len());
    let chars: Vec<char> = src.chars().collect();
    let len = chars.len();
    let mut i = 0;
    while i < len {
        if i + 1 < len && chars[i] == '/' && chars[i + 1] == '*' {
            i += 2;
            while i + 1 < len {
                if chars[i] == '*' && chars[i + 1] == '/' {
                    i += 2;
                    break;
                }
                i += 1;
            }
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }
    result
}

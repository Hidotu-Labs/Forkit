use super::selector::*;
use super::utils::*;

/// A single CSS rule: one or more selectors paired with a block of declarations.
#[derive(Debug, Clone)]
pub struct Rule {
    /// Parsed selectors for this rule.
    pub selectors: Vec<Selector>,
    /// `(property, value)` pairs parsed from the declaration block.
    pub declarations: Vec<(String, String)>,
}

/// A parsed CSS stylesheet containing zero or more rules.
#[derive(Debug, Clone, Default)]
pub struct StyleSheet {
    pub rules: Vec<Rule>,
}

impl StyleSheet {
    /// Parse a CSS source string into a `StyleSheet`.
    pub fn parse(css: &str) -> StyleSheet {
        let mut rules = Vec::new();

        // Step 1: strip block comments
        let stripped = strip_comments(css);

        let mut pos = 0;
        let chars: Vec<char> = stripped.chars().collect();
        let len = chars.len();

        while pos < len {
            // Skip whitespace
            skip_whitespace(&chars, &mut pos);
            if pos >= len {
                break;
            }

            // Check for at-rules
            if chars[pos] == '@' {
                skip_at_rule(&chars, &mut pos);
                continue;
            }

            // Collect selector string up to `{`
            let selector_start = pos;
            let mut found_open = false;
            let mut paren_depth_sel = 0usize;
            while pos < len {
                match chars[pos] {
                    '(' => { paren_depth_sel += 1; pos += 1; }
                    ')' => { if paren_depth_sel > 0 { paren_depth_sel -= 1; } pos += 1; }
                    '{' if paren_depth_sel == 0 => { found_open = true; break; }
                    _ => { pos += 1; }
                }
            }

            if !found_open {
                break;
            }

            let selector_str: String = chars[selector_start..pos].iter().collect();
            let selector_str = selector_str.trim();

            if selector_str.is_empty() {
                pos += 1; // skip `{`
                skip_to_closing_brace(&chars, &mut pos);
                continue;
            }

            // Parse comma-separated selectors
            let selectors: Vec<Selector> = split_selector_list(selector_str)
                .into_iter()
                .filter_map(|s| parse_selector(&s))
                .collect();

            // Advance past `{`
            pos += 1;

            // Collect declaration block up to `}`
            let decl_start = pos;
            let mut found_close = false;
            let mut in_quote: Option<char> = None;
            let mut paren_depth = 0usize;
            while pos < len {
                let c = chars[pos];
                match in_quote {
                    Some(q) if c == q => { in_quote = None; }
                    Some('\\') => { pos += 1; }
                    Some(_) => {}
                    None => match c {
                        '"' | '\'' => { in_quote = Some(c); }
                        '(' => { paren_depth += 1; }
                        ')' => { if paren_depth > 0 { paren_depth -= 1; } }
                        '}' if paren_depth == 0 => { found_close = true; break; }
                        _ => {}
                    }
                }
                pos += 1;
            }

            if !found_close {
                eprintln!("CSS parse error: rule for {:?} has no closing `}}`", selector_str);
                break;
            }

            let decl_block: String = chars[decl_start..pos].iter().collect();

            pos += 1; // advance past `}`

            let declarations = parse_declarations(&decl_block, selector_str);

            rules.push(Rule { selectors, declarations });
        }

        StyleSheet { rules }
    }
}

/// Parse the text between `{` and `}` into `(property, value)` pairs.
pub fn parse_declarations(block: &str, _selector: &str) -> Vec<(String, String)> {
    let mut result = Vec::new();
    let chars: Vec<char> = block.chars().collect();
    let len = chars.len();
    let mut pos = 0;

    while pos < len {
        let decl_start = pos;
        let mut in_quote: Option<char> = None;
        let mut paren_depth = 0usize;

        while pos < len {
            let c = chars[pos];
            match in_quote {
                Some(q) if c == q => { in_quote = None; }
                Some(_) => {}
                None => match c {
                    '"' | '\'' => { in_quote = Some(c); }
                    '(' => { paren_depth += 1; }
                    ')' => { if paren_depth > 0 { paren_depth -= 1; } }
                    ';' if paren_depth == 0 => { break; }
                    _ => {}
                }
            }
            pos += 1;
        }

        let decl: String = chars[decl_start..pos].iter().collect();
        let decl = decl.trim();
        if pos < len { pos += 1; } // advance past `;`

        if decl.is_empty() { continue; }

        let decl_chars: Vec<char> = decl.chars().collect();
        let mut colon_pos = None;
        let mut iq: Option<char> = None;
        let mut pd = 0usize;
        for (idx, &c) in decl_chars.iter().enumerate() {
            match iq {
                Some(q) if c == q => { iq = None; }
                Some(_) => {}
                None => match c {
                    '"' | '\'' => { iq = Some(c); }
                    '(' => { pd += 1; }
                    ')' => { if pd > 0 { pd -= 1; } }
                    ':' if pd == 0 => { colon_pos = Some(idx); break; }
                    _ => {}
                }
            }
        }

        match colon_pos {
            Some(cp) => {
                let property = decl[..cp].trim().to_string();
                let value    = decl[cp + 1..].trim().to_string();
                if property.is_empty() {
                    continue;
                }
                result.push((property, value));
            }
            None => {}
        }
    }

    result
}

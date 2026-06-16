use super::selector::*;
use super::utils::*;
use crate::dom::css::inline;

/// A single CSS rule: one or more selectors paired with a block of declarations.
#[derive(Debug, Clone)]
pub struct Rule {
    /// Parsed selectors for this rule.
    pub selectors: Vec<Selector>,
    /// `(property, value)` pairs parsed from the declaration block.
    pub declarations: Vec<(String, String)>,
}

/// A rule for custom fonts.
#[derive(Debug, Clone, Default)]
pub struct FontFaceRule {
    pub family: String,
    pub srcs:   Vec<String>,
    pub bold:   bool,
    pub italic: bool,
}

/// A parsed CSS stylesheet containing zero or more rules and @font-face definitions.
#[derive(Debug, Clone, Default)]
pub struct StyleSheet {
    pub rules:      Vec<Rule>,
    pub font_faces: Vec<FontFaceRule>,
    /// Rule index for fast cascade lookup.
    /// Maps a subject key (lowercase tag name, ".classname", "#id", or "*") to
    /// the indices of rules in `self.rules` whose subject selector could match
    /// an element with that key.  Built once after parsing.
    pub(crate) index: std::collections::HashMap<String, Vec<usize>>,
}

/// Returns the subject key for a selector as an owned String.
pub(crate) fn selector_subject_key_owned(sel: &Selector) -> String {
    match sel {
        Selector::Tag(t) => t.to_ascii_lowercase(),
        Selector::Class(c) => format!(".{}", c),
        Selector::Id(id)   => format!("#{}", id),
        Selector::Universal => "*".to_owned(),
        Selector::Compound(parts) => {
            // Extract the most specific narrow key from the compound parts.
            // Priority: id > class > tag > universal/attr/pseudo
            let mut tag_key: Option<String>   = None;
            let mut class_key: Option<String> = None;
            let mut id_key: Option<String>    = None;
            for ss in parts {
                match ss {
                    SimpleSelector::Id(id)    => { id_key = Some(format!("#{id}")); break; }
                    SimpleSelector::Class(c)  => { if class_key.is_none() { class_key = Some(format!(".{c}")); } }
                    SimpleSelector::Tag(t)    => { if tag_key.is_none() { tag_key = Some(t.to_ascii_lowercase()); } }
                    _ => {}
                }
            }
            id_key.or(class_key).or(tag_key).unwrap_or_else(|| "*".to_owned())
        }
        // Combinators: subject is the right-hand side selector.
        Selector::Descendant(_, b)
        | Selector::Child(_, b)
        | Selector::AdjacentSibling(_, b)
        | Selector::GeneralSibling(_, b) => selector_subject_key_owned(b),
    }
}

impl StyleSheet {
    /// Parse a CSS source string into a `StyleSheet`.
    pub fn parse(css: &str) -> StyleSheet {
        let mut rules = Vec::new();
        let mut font_faces = Vec::new();

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
                let rule_start = pos;
                skip_at_rule_name(&chars, &mut pos);
                let name: String = chars[rule_start + 1..pos].iter().collect();

                if name == "font-face" {
                    skip_whitespace(&chars, &mut pos);
                    if pos < len && chars[pos] == '{' {
                        pos += 1;
                        let inner_start = pos;
                        skip_to_closing_brace(&chars, &mut pos);
                        let inner_block: String = chars[inner_start..pos-1].iter().collect();
                        let decls = parse_declarations(&inner_block, "@font-face");

                        let mut face = FontFaceRule::default();
                        for (p, v) in decls {
                            match p.as_str() {
                                "font-family" => face.family = v.trim_matches(|c| c == '"' || c == '\'').to_string(),
                                "src" => {
                                    face.srcs = inline::extract_css_urls(&v);
                                }
                                "font-weight" => face.bold = v.contains("bold") || v.contains("700"),
                                "font-style"  => face.italic = v.contains("italic"),
                                _ => {}
                            }
                        }
                        if !face.family.is_empty() && !face.srcs.is_empty() {
                            font_faces.push(face);
                        }
                        continue;
                    }
                }

                // If not @font-face or failed, use generic skip
                pos = rule_start;
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

        // Build the rule index: map subject key → rule indices.
        let mut index: std::collections::HashMap<String, Vec<usize>> = std::collections::HashMap::new();
        for (rule_idx, rule) in rules.iter().enumerate() {
            // A rule can have multiple selectors (comma-separated). Insert the rule
            // under every unique subject key among its selectors so we never miss it.
            let mut keys_seen = std::collections::HashSet::new();
            for sel in &rule.selectors {
                let key = selector_subject_key_owned(sel);
                if keys_seen.insert(key.clone()) {
                    index.entry(key).or_default().push(rule_idx);
                }
            }
        }

        StyleSheet { rules, font_faces, index }
    }

    /// Return the rule indices that could potentially match an element with
    /// the given tag, id, and space-separated class list.
    /// Merges: tag bucket + each class bucket + id bucket + universal bucket.
    /// The returned vec may contain duplicates if a rule has selectors in
    /// multiple buckets — callers must de-duplicate or use a seen-set.
    pub(crate) fn candidate_rule_indices(
        &self,
        tag:        &str,
        id:         &str,
        class_name: &str,
    ) -> Vec<usize> {
        let mut out = Vec::new();
        let tag_lc = tag.to_ascii_lowercase();

        // Tag bucket
        if let Some(v) = self.index.get(&tag_lc) { out.extend_from_slice(v); }
        // Class buckets — one per whitespace-separated token
        for cls in class_name.split_ascii_whitespace() {
            let k = format!(".{cls}");
            if let Some(v) = self.index.get(&k) { out.extend_from_slice(v); }
        }
        // Id bucket
        if !id.is_empty() {
            let k = format!("#{id}");
            if let Some(v) = self.index.get(&k) { out.extend_from_slice(v); }
        }
        // Universal / combinator / attr / pseudo bucket
        if let Some(v) = self.index.get("*") { out.extend_from_slice(v); }

        out
    }
}

fn skip_at_rule_name(chars: &[char], pos: &mut usize) {
    *pos += 1; // skip '@'
    while *pos < chars.len() && (chars[*pos].is_alphanumeric() || chars[*pos] == '-') {
        *pos += 1;
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

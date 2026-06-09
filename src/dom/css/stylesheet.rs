/// CSS stylesheet tokeniser and rule parser.
///
/// Parses a CSS source string into a `StyleSheet` containing `Rule` values.
/// Each `Rule` holds a list of selectors and `(property, value)` declaration pairs.
///
/// Notes:
/// - `/* … */` block comments are stripped before parsing.
/// - `@import` at-rules are silently skipped (network fetching is out of scope).
/// - On any parse error in a rule the parser logs with `eprintln!` and skips to the next `}`.
/// - The parser never panics.

/// A simple selector component, used inside a `Selector::Compound`.
#[derive(Debug, Clone, PartialEq)]
pub enum SimpleSelector {
    /// Matches elements by tag name, e.g. `div`.
    Tag(String),
    /// Matches elements with the given class, e.g. `.foo`.
    Class(String),
    /// Matches the element with the given id, e.g. `#bar`.
    Id(String),
    /// Matches any element (`*`).
    Universal,
    /// Matches elements that have the named attribute, e.g. `[href]`.
    AttrPresence(String),
    /// Matches elements where attribute equals a value, e.g. `[type="text"]`.
    AttrEquality(String, String),
}

/// A CSS selector, potentially combining simple selectors with combinators.
#[derive(Debug, Clone, PartialEq)]
pub enum Selector {
    /// Matches elements by tag name, e.g. `div`.
    Tag(String),
    /// Matches elements with the given class, e.g. `.foo`.
    Class(String),
    /// Matches the element with the given id, e.g. `#bar`.
    Id(String),
    /// Matches any element (`*`).
    Universal,
    /// A compound selector: all components must match the same element simultaneously.
    Compound(Vec<SimpleSelector>),
    /// Descendant combinator (`A B`): `B` is a descendant of `A`.
    Descendant(Box<Selector>, Box<Selector>),
    /// Child combinator (`A > B`): `B` is a direct child of `A`.
    Child(Box<Selector>, Box<Selector>),
    /// Adjacent sibling combinator (`A + B`): `B` immediately follows `A`.
    AdjacentSibling(Box<Selector>, Box<Selector>),
}

/// CSS specificity, represented as `(id, class, tag)` counts.
///
/// Higher values take precedence; compare left-to-right (id first).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Specificity(pub u32, pub u32, pub u32);

/// A parsed CSS stylesheet containing zero or more rules.
#[derive(Debug, Clone, Default)]
pub struct StyleSheet {
    pub rules: Vec<Rule>,
}

/// A single CSS rule: one or more selectors paired with a block of declarations.
#[derive(Debug, Clone)]
pub struct Rule {
    /// Parsed selectors for this rule.
    pub selectors: Vec<Selector>,
    /// `(property, value)` pairs parsed from the declaration block.
    pub declarations: Vec<(String, String)>,
}

impl Selector {
    /// Compute the CSS specificity of this selector as `(id_count, class_count, tag_count)`.
    ///
    /// Rules:
    /// - Each `#id` contributes `(1, 0, 0)`.
    /// - Each `.class`, attribute presence/equality contributes `(0, 1, 0)`.
    /// - Each tag name contributes `(0, 0, 1)`.
    /// - `*` (universal) contributes nothing.
    /// - Combinators sum the specificity of both sides.
    /// - `Compound` sums the specificity of every `SimpleSelector` inside it.
    pub fn specificity(&self) -> Specificity {
        match self {
            Selector::Tag(_) => Specificity(0, 0, 1),
            Selector::Class(_) => Specificity(0, 1, 0),
            Selector::Id(_) => Specificity(1, 0, 0),
            Selector::Universal => Specificity(0, 0, 0),
            Selector::Compound(parts) => {
                let mut ids = 0u32;
                let mut classes = 0u32;
                let mut tags = 0u32;
                for ss in parts {
                    match ss {
                        SimpleSelector::Id(_) => ids += 1,
                        SimpleSelector::Class(_)
                        | SimpleSelector::AttrPresence(_)
                        | SimpleSelector::AttrEquality(_, _) => classes += 1,
                        SimpleSelector::Tag(_) => tags += 1,
                        SimpleSelector::Universal => {}
                    }
                }
                Specificity(ids, classes, tags)
            }
            Selector::Descendant(a, b)
            | Selector::Child(a, b)
            | Selector::AdjacentSibling(a, b) => {
                let Specificity(ai, ac, at) = a.specificity();
                let Specificity(bi, bc, bt) = b.specificity();
                Specificity(ai + bi, ac + bc, at + bt)
            }
        }
    }
}

/// Parse a comma-separated selector list string into a `Vec<Selector>`.
///
/// Each comma-separated segment is parsed as a selector group.
/// Invalid or empty segments are silently skipped.
///
/// # Example
/// ```text
/// parse_selector_list("h1, h2, .foo") → [Tag("h1"), Tag("h2"), Class("foo")]
/// ```
pub fn parse_selector_list(s: &str) -> Vec<Selector> {
    s.split(',')
        .map(|seg| seg.trim())
        .filter(|seg| !seg.is_empty())
        .filter_map(parse_selector_group)
        .collect()
}

/// Parse a single selector group string (no commas) into a `Selector`.
///
/// Handles descendant (space), child (`>`), and adjacent sibling (`+`) combinators,
/// left-to-right, building a tree of `Selector` variants.
/// Returns `None` for empty or completely unparseable input.
///
/// This is the public counterpart to the internal `parse_selector` helper.
pub fn parse_selector_group(s: &str) -> Option<Selector> {
    parse_selector(s)
}

/// Parse all simple selector components out of a compound selector token
/// (a token that contains no combinators), e.g. `div.foo#bar[href]`.
///
/// Returns a `Vec<SimpleSelector>` — one entry per component.
/// An empty vec is returned for empty input.
///
/// This is the public counterpart to the internal `split_simple_selectors` helper.
pub fn parse_simple_selector(token: &str) -> Vec<SimpleSelector> {
    split_simple_selectors(token)
}

impl StyleSheet {
    /// Parse a CSS source string into a `StyleSheet`.
    ///
    /// The tokeniser works character-by-character:
    /// 1. Strip block comments (`/* … */`).
    /// 2. Skip `@import` lines silently.
    /// 3. Read until `{` to collect the selector string.
    /// 4. Parse the selector string as comma-separated strings (trim each).
    /// 5. Read until `}` collecting `property: value` pairs split on `;`.
    /// 6. Skip empty declarations.
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
                // Read to end of this at-rule (up to `;` or `{...}`)
                skip_at_rule(&chars, &mut pos);
                continue;
            }

            // Collect selector string up to `{`
            let selector_start = pos;
            let mut found_open = false;
            while pos < len {
                if chars[pos] == '{' {
                    found_open = true;
                    break;
                }
                pos += 1;
            }

            if !found_open {
                // Trailing text with no rule block — nothing to do
                break;
            }

            let selector_str: String = chars[selector_start..pos].iter().collect();
            let selector_str = selector_str.trim();

            // Skip empty selector
            if selector_str.is_empty() {
                // Advance past `{` and skip to `}`
                pos += 1; // skip `{`
                skip_to_closing_brace(&chars, &mut pos);
                continue;
            }

            // Parse comma-separated selectors
            let selectors: Vec<Selector> = selector_str
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .filter_map(|s| parse_selector(s))
                .collect();

            // Advance past `{`
            pos += 1;

            // Collect declaration block up to `}`, skipping over quoted strings
            // and url(...) so that '}' inside data URIs doesn't close the block early.
            let decl_start = pos;
            let mut found_close = false;
            let mut in_quote: Option<char> = None;
            let mut paren_depth = 0usize;
            while pos < len {
                let c = chars[pos];
                match in_quote {
                    Some(q) if c == q => { in_quote = None; }
                    Some('\\') => { pos += 1; } // skip escaped char (already advanced below)
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
                // Malformed: no closing `}` — log and stop
                eprintln!("CSS parse error: rule for {:?} has no closing `}}`", selector_str);
                break;
            }

            let decl_block: String = chars[decl_start..pos].iter().collect();

            // Advance past `}`
            pos += 1;

            // Parse declarations
            let declarations = parse_declarations(&decl_block, selector_str);

            rules.push(Rule { selectors, declarations });
        }

        StyleSheet { rules }
    }
}

/// Remove `/* … */` block comments from the source string.
fn strip_comments(src: &str) -> String {
    let mut result = String::with_capacity(src.len());
    let chars: Vec<char> = src.chars().collect();
    let len = chars.len();
    let mut i = 0;
    while i < len {
        if i + 1 < len && chars[i] == '/' && chars[i + 1] == '*' {
            // Skip until `*/`
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

/// Skip whitespace characters in `chars` starting at `pos`.
fn skip_whitespace(chars: &[char], pos: &mut usize) {
    while *pos < chars.len() && chars[*pos].is_whitespace() {
        *pos += 1;
    }
}

/// Skip an at-rule starting at `@`.
/// Handles two forms:
/// - Simple at-rule ending with `;` (e.g. `@import "…";`)
/// - Block at-rule ending with `{…}` (e.g. `@media { … }`)
fn skip_at_rule(chars: &[char], pos: &mut usize) {
    // Advance past `@`
    *pos += 1;
    while *pos < chars.len() {
        match chars[*pos] {
            ';' => {
                *pos += 1;
                return;
            }
            '{' => {
                // Skip the block
                *pos += 1;
                skip_to_closing_brace(chars, pos);
                return;
            }
            _ => {
                *pos += 1;
            }
        }
    }
}

/// Advance `pos` past the next `}` (the matching close of a rule block).
/// Assumes we are positioned inside or just before the `}`.
fn skip_to_closing_brace(chars: &[char], pos: &mut usize) {
    let mut depth = 1usize;
    while *pos < chars.len() {
        match chars[*pos] {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    *pos += 1;
                    return;
                }
            }
            _ => {}
        }
        *pos += 1;
    }
}

/// Parse a single selector string (no commas) into a `Selector`.
///
/// Handles combinators (descendant whitespace, `>` child, `+` adjacent sibling)
/// and compound selectors (multiple simple selectors on the same element).
/// Returns `None` if the string is empty or completely unparseable.
fn parse_selector(s: &str) -> Option<Selector> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    // Tokenise the selector into tokens split by combinators.
    // We walk character-by-character to handle bracket content (`[attr=val]`) correctly.
    #[derive(Debug)]
    enum CombinatorKind {
        Descendant,
        Child,
        AdjacentSibling,
    }

    let chars: Vec<char> = s.chars().collect();
    let len = chars.len();
    let mut i = 0;

    // Each entry: (combinator from previous, simple-selector token)
    let mut parts: Vec<(Option<CombinatorKind>, String)> = Vec::new();
    let mut current_token = String::new();
    let mut pending_combinator: Option<CombinatorKind> = None;
    let mut saw_whitespace = false;

    while i < len {
        match chars[i] {
            // Attribute selector — grab everything up to `]`
            '[' => {
                current_token.push('[');
                i += 1;
                while i < len && chars[i] != ']' {
                    current_token.push(chars[i]);
                    i += 1;
                }
                if i < len {
                    current_token.push(']');
                    i += 1;
                }
            }
            '>' => {
                if !current_token.trim().is_empty() {
                    parts.push((pending_combinator.take(), current_token.trim().to_string()));
                    current_token.clear();
                }
                pending_combinator = Some(CombinatorKind::Child);
                saw_whitespace = false;
                i += 1;
            }
            '+' => {
                if !current_token.trim().is_empty() {
                    parts.push((pending_combinator.take(), current_token.trim().to_string()));
                    current_token.clear();
                }
                pending_combinator = Some(CombinatorKind::AdjacentSibling);
                saw_whitespace = false;
                i += 1;
            }
            c if c.is_whitespace() => {
                saw_whitespace = true;
                i += 1;
            }
            c => {
                // If we had whitespace and no explicit combinator, and a token was building,
                // flush it as a new compound unit with a Descendant combinator.
                if saw_whitespace && !current_token.trim().is_empty() && pending_combinator.is_none() {
                    parts.push((None, current_token.trim().to_string()));
                    current_token.clear();
                    pending_combinator = Some(CombinatorKind::Descendant);
                }
                saw_whitespace = false;
                current_token.push(c);
                i += 1;
            }
        }
    }

    // Flush the last token
    if !current_token.trim().is_empty() {
        parts.push((pending_combinator.take(), current_token.trim().to_string()));
    }

    if parts.is_empty() {
        return None;
    }

    // Parse each token into a Selector leaf, then fold with combinators
    let mut result: Option<Selector> = None;
    for (combinator, token) in parts {
        let right = parse_compound_selector(&token)?;
        result = Some(match (result, combinator) {
            (None, _) => right,
            (Some(left), Some(CombinatorKind::Descendant)) => {
                Selector::Descendant(Box::new(left), Box::new(right))
            }
            (Some(left), Some(CombinatorKind::Child)) => {
                Selector::Child(Box::new(left), Box::new(right))
            }
            (Some(left), Some(CombinatorKind::AdjacentSibling)) => {
                Selector::AdjacentSibling(Box::new(left), Box::new(right))
            }
            // Implicit descendant when no explicit combinator stored but we have a left
            (Some(left), None) => {
                Selector::Descendant(Box::new(left), Box::new(right))
            }
        });
    }

    result
}

/// Parse a compound selector token (no combinators) like `div.foo#bar[type="text"]`
/// into a `Selector`.
///
/// - A single simple component is unwrapped (returns `Tag`, `Class`, `Id`, or `Universal`).
/// - Multiple components are wrapped in `Selector::Compound`.
fn parse_compound_selector(token: &str) -> Option<Selector> {
    let token = token.trim();
    if token.is_empty() {
        return None;
    }

    let components = split_simple_selectors(token);
    if components.is_empty() {
        return None;
    }

    // If there is exactly one component, return the canonical single-variant
    if components.len() == 1 {
        return match &components[0] {
            SimpleSelector::Tag(t) => Some(Selector::Tag(t.clone())),
            SimpleSelector::Class(c) => Some(Selector::Class(c.clone())),
            SimpleSelector::Id(id) => Some(Selector::Id(id.clone())),
            SimpleSelector::Universal => Some(Selector::Universal),
            _ => Some(Selector::Compound(components)),
        };
    }

    Some(Selector::Compound(components))
}

/// Split a compound-selector token like `div.foo#bar[href]` into individual `SimpleSelector`s.
fn split_simple_selectors(token: &str) -> Vec<SimpleSelector> {
    let mut result = Vec::new();
    let chars: Vec<char> = token.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        match chars[i] {
            '.' => {
                i += 1;
                let name = take_ident(&chars, &mut i);
                if !name.is_empty() {
                    result.push(SimpleSelector::Class(name));
                }
            }
            '#' => {
                i += 1;
                let name = take_ident(&chars, &mut i);
                if !name.is_empty() {
                    result.push(SimpleSelector::Id(name));
                }
            }
            '*' => {
                result.push(SimpleSelector::Universal);
                i += 1;
            }
            '[' => {
                // Attribute selector
                i += 1; // skip `[`
                let attr_content = take_until(&chars, &mut i, ']');
                i += 1; // skip `]`
                if let Some(eq_pos) = attr_content.find('=') {
                    let attr = attr_content[..eq_pos].trim().to_string();
                    let val = attr_content[eq_pos + 1..]
                        .trim()
                        .trim_matches('"')
                        .trim_matches('\'')
                        .to_string();
                    result.push(SimpleSelector::AttrEquality(attr, val));
                } else {
                    let attr = attr_content.trim().to_string();
                    if !attr.is_empty() {
                        result.push(SimpleSelector::AttrPresence(attr));
                    }
                }
            }
            _ => {
                // Tag name
                let name = take_ident(&chars, &mut i);
                if !name.is_empty() {
                    result.push(SimpleSelector::Tag(name));
                } else {
                    // Unrecognised character — skip
                    i += 1;
                }
            }
        }
    }

    result
}

/// Collect identifier characters (alphanumeric, `-`, `_`) from `chars` starting at `*pos`.
fn take_ident(chars: &[char], pos: &mut usize) -> String {
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

/// Collect all characters up to (but not including) `stop`, advancing `pos`.
fn take_until(chars: &[char], pos: &mut usize, stop: char) -> String {
    let mut s = String::new();
    while *pos < chars.len() && chars[*pos] != stop {
        s.push(chars[*pos]);
        *pos += 1;
    }
    s
}

/// Parse the text between `{` and `}` into `(property, value)` pairs.
/// Correctly handles semicolons inside url(...) and quoted strings.
fn parse_declarations(block: &str, _selector: &str) -> Vec<(String, String)> {
    let mut result = Vec::new();
    let chars: Vec<char> = block.chars().collect();
    let len = chars.len();
    let mut pos = 0;

    while pos < len {
        // Collect one declaration, respecting quotes and parens
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
        // Advance past the ';'
        if pos < len { pos += 1; }

        if decl.is_empty() { continue; }

        // Find the first ':' that is not inside parens or quotes
        let decl_chars: Vec<char> = decl.chars().collect();
        let mut colon_pos = None;
        let mut iq: Option<char> = None;
        let mut pd = 0usize;
        for (i, &c) in decl_chars.iter().enumerate() {
            match iq {
                Some(q) if c == q => { iq = None; }
                Some(_) => {}
                None => match c {
                    '"' | '\'' => { iq = Some(c); }
                    '(' => { pd += 1; }
                    ')' => { if pd > 0 { pd -= 1; } }
                    ':' if pd == 0 => { colon_pos = Some(i); break; }
                    _ => {}
                }
            }
        }

        match colon_pos {
            Some(cp) => {
                let property = decl[..cp].trim().to_string();
                let value    = decl[cp + 1..].trim().to_string();
                if property.is_empty() {
                    // silent skip — pseudo-selectors like ::before produce these
                    continue;
                }
                result.push((property, value));
            }
            None => {
                // No colon — skip silently (avoids flooding stderr with data URIs)
            }
        }
    }

    result
}

// ──────────────────────────────────────────────────────────────────────────────
// Unit tests
// ──────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    /// empty_input: `StyleSheet::parse("")` → 0 rules
    #[test]
    fn empty_input() {
        let ss = StyleSheet::parse("");
        assert_eq!(ss.rules.len(), 0);
    }

    /// single_rule: one declaration parsed correctly
    #[test]
    fn single_rule() {
        let ss = StyleSheet::parse("p { color: red; }");
        assert_eq!(ss.rules.len(), 1);
        let rule = &ss.rules[0];
        assert_eq!(rule.selectors, vec![Selector::Tag("p".to_string())]);
        assert_eq!(rule.declarations.len(), 1);
        assert_eq!(rule.declarations[0], ("color".to_string(), "red".to_string()));
    }

    /// multiple_selectors: comma-separated selectors produce one rule with two selectors
    #[test]
    fn multiple_selectors() {
        let ss = StyleSheet::parse("h1, h2 { font-weight: bold; }");
        assert_eq!(ss.rules.len(), 1);
        let rule = &ss.rules[0];
        assert_eq!(
            rule.selectors,
            vec![
                Selector::Tag("h1".to_string()),
                Selector::Tag("h2".to_string()),
            ]
        );
        assert_eq!(rule.declarations.len(), 1);
        assert_eq!(rule.declarations[0], ("font-weight".to_string(), "bold".to_string()));
    }

    /// syntax_error_recovery: a malformed rule followed by a valid rule
    /// must not panic; at minimum the valid rule is parsed.
    #[test]
    fn syntax_error_recovery() {
        // Rule with a declaration missing `:` (malformed), followed by a valid rule.
        let css = "div { no-colon-here } p { color: blue; }";
        let ss = StyleSheet::parse(css);
        // The parser should not panic. It should produce at least the valid `p` rule.
        assert!(!ss.rules.is_empty(), "expected at least one rule");
        // The `p` rule must be present somewhere.
        let p_rule = ss.rules.iter().find(|r| {
            r.selectors.iter().any(|s| matches!(s, Selector::Tag(t) if t == "p"))
        });
        assert!(p_rule.is_some(), "valid `p` rule should be present after error recovery");
        let p_rule = p_rule.unwrap();
        assert_eq!(p_rule.declarations, vec![("color".to_string(), "blue".to_string())]);
    }

    /// import_ignored: `@import` silently skipped; following rules parsed normally
    #[test]
    fn import_ignored() {
        let ss = StyleSheet::parse("@import \"style.css\"; p { color: red; }");
        // The import is skipped; only the `p` rule is produced.
        assert_eq!(ss.rules.len(), 1);
        let rule = &ss.rules[0];
        assert_eq!(rule.selectors, vec![Selector::Tag("p".to_string())]);
        assert_eq!(rule.declarations[0], ("color".to_string(), "red".to_string()));
    }

    /// block_comment: comments are stripped before parsing
    #[test]
    fn block_comment_stripped() {
        let ss = StyleSheet::parse("/* this is a comment */ p { color: green; }");
        assert_eq!(ss.rules.len(), 1);
        assert_eq!(ss.rules[0].declarations[0], ("color".to_string(), "green".to_string()));
    }

    /// multiple_declarations: multiple property-value pairs in one rule
    #[test]
    fn multiple_declarations() {
        let css = "a { color: blue; text-decoration: underline; font-size: 14px; }";
        let ss = StyleSheet::parse(css);
        assert_eq!(ss.rules.len(), 1);
        let decls = &ss.rules[0].declarations;
        assert_eq!(decls.len(), 3);
        assert_eq!(decls[0], ("color".to_string(), "blue".to_string()));
        assert_eq!(decls[1], ("text-decoration".to_string(), "underline".to_string()));
        assert_eq!(decls[2], ("font-size".to_string(), "14px".to_string()));
    }

    /// import_url: `@import url(…)` form also silently skipped
    #[test]
    fn import_url_ignored() {
        let ss = StyleSheet::parse("@import url(reset.css); h1 { font-size: 2em; }");
        assert_eq!(ss.rules.len(), 1);
        assert_eq!(ss.rules[0].selectors, vec![Selector::Tag("h1".to_string())]);
    }

    // ── Selector parsing tests ──────────────────────────────────────────────

    /// parse_selector_tag: a plain tag name produces `Selector::Tag`
    #[test]
    fn parse_selector_tag() {
        let sel = parse_selector("div");
        assert_eq!(sel, Some(Selector::Tag("div".to_string())));
    }

    /// parse_selector_class: `.foo` produces `Selector::Class`
    #[test]
    fn parse_selector_class() {
        let sel = parse_selector(".foo");
        assert_eq!(sel, Some(Selector::Class("foo".to_string())));
    }

    /// parse_selector_id: `#bar` produces `Selector::Id`
    #[test]
    fn parse_selector_id() {
        let sel = parse_selector("#bar");
        assert_eq!(sel, Some(Selector::Id("bar".to_string())));
    }

    /// parse_selector_universal: `*` produces `Selector::Universal`
    #[test]
    fn parse_selector_universal() {
        let sel = parse_selector("*");
        assert_eq!(sel, Some(Selector::Universal));
    }

    /// parse_selector_compound: `div.foo` produces `Selector::Compound`
    #[test]
    fn parse_selector_compound() {
        let sel = parse_selector("div.foo");
        assert_eq!(
            sel,
            Some(Selector::Compound(vec![
                SimpleSelector::Tag("div".to_string()),
                SimpleSelector::Class("foo".to_string()),
            ]))
        );
    }

    /// parse_selector_descendant: `div p` produces `Selector::Descendant`
    #[test]
    fn parse_selector_descendant() {
        let sel = parse_selector("div p");
        assert_eq!(
            sel,
            Some(Selector::Descendant(
                Box::new(Selector::Tag("div".to_string())),
                Box::new(Selector::Tag("p".to_string())),
            ))
        );
    }

    /// parse_selector_child: `ul > li` produces `Selector::Child`
    #[test]
    fn parse_selector_child() {
        let sel = parse_selector("ul > li");
        assert_eq!(
            sel,
            Some(Selector::Child(
                Box::new(Selector::Tag("ul".to_string())),
                Box::new(Selector::Tag("li".to_string())),
            ))
        );
    }

    /// parse_selector_adjacent: `h1 + p` produces `Selector::AdjacentSibling`
    #[test]
    fn parse_selector_adjacent() {
        let sel = parse_selector("h1 + p");
        assert_eq!(
            sel,
            Some(Selector::AdjacentSibling(
                Box::new(Selector::Tag("h1".to_string())),
                Box::new(Selector::Tag("p".to_string())),
            ))
        );
    }

    /// parse_selector_attr_presence: `[href]` produces `AttrPresence`
    #[test]
    fn parse_selector_attr_presence() {
        let sel = parse_selector("[href]");
        assert_eq!(
            sel,
            Some(Selector::Compound(vec![SimpleSelector::AttrPresence("href".to_string())]))
        );
    }

    /// parse_selector_attr_equality: `[type="text"]` produces `AttrEquality`
    #[test]
    fn parse_selector_attr_equality() {
        let sel = parse_selector("[type=\"text\"]");
        assert_eq!(
            sel,
            Some(Selector::Compound(vec![SimpleSelector::AttrEquality(
                "type".to_string(),
                "text".to_string(),
            )]))
        );
    }

    // ── Specificity tests ───────────────────────────────────────────────────

    /// specificity_ordering: id > class > tag
    #[test]
    fn specificity_ordering() {
        let tag = Specificity(0, 0, 1);
        let class = Specificity(0, 1, 0);
        let id = Specificity(1, 0, 0);
        assert!(tag < class);
        assert!(class < id);
        assert!(tag < id);
    }

    /// specificity_equal: two identical specificities compare equal
    #[test]
    fn specificity_equal() {
        assert_eq!(Specificity(1, 2, 3), Specificity(1, 2, 3));
    }

    // ── Task 6.6 tests ──────────────────────────────────────────────────────

    /// specificity_compound: `div.foo#bar` → (1, 1, 1)
    #[test]
    fn specificity_compound_div_foo_bar() {
        let sel = parse_selector_group("div.foo#bar").expect("should parse");
        assert_eq!(sel.specificity(), Specificity(1, 1, 1));
    }

    /// specificity_id_only: `#main` → (1, 0, 0)
    #[test]
    fn specificity_id_only() {
        let sel = parse_selector_group("#main").expect("should parse");
        assert_eq!(sel.specificity(), Specificity(1, 0, 0));
    }

    /// specificity_class_only: `.btn` → (0, 1, 0)
    #[test]
    fn specificity_class_only() {
        let sel = parse_selector_group(".btn").expect("should parse");
        assert_eq!(sel.specificity(), Specificity(0, 1, 0));
    }

    /// specificity_tag_only: `p` → (0, 0, 1)
    #[test]
    fn specificity_tag_only() {
        let sel = parse_selector_group("p").expect("should parse");
        assert_eq!(sel.specificity(), Specificity(0, 0, 1));
    }

    /// specificity_universal: `*` → (0, 0, 0)
    #[test]
    fn specificity_universal() {
        let sel = parse_selector_group("*").expect("should parse");
        assert_eq!(sel.specificity(), Specificity(0, 0, 0));
    }

    /// child_combinator: `a > b` produces `Selector::Child`
    #[test]
    fn child_combinator_a_gt_b() {
        let sel = parse_selector_group("a > b").expect("should parse");
        assert_eq!(
            sel,
            Selector::Child(
                Box::new(Selector::Tag("a".to_string())),
                Box::new(Selector::Tag("b".to_string())),
            )
        );
    }

    /// child_combinator_specificity: `a > b` → (0, 0, 2)
    #[test]
    fn child_combinator_specificity() {
        let sel = parse_selector_group("a > b").expect("should parse");
        assert_eq!(sel.specificity(), Specificity(0, 0, 2));
    }

    /// attr_presence: `[href]` produces `AttrPresence` via parse_simple_selector
    #[test]
    fn parse_simple_selector_attr_presence() {
        let parts = parse_simple_selector("[href]");
        assert_eq!(parts, vec![SimpleSelector::AttrPresence("href".to_string())]);
    }

    /// attr_equality: `[type="text"]` produces `AttrEquality`
    #[test]
    fn parse_simple_selector_attr_equality() {
        let parts = parse_simple_selector("[type=\"text\"]");
        assert_eq!(
            parts,
            vec![SimpleSelector::AttrEquality("type".to_string(), "text".to_string())]
        );
    }

    /// parse_selector_list_splits_on_comma
    #[test]
    fn parse_selector_list_splits_on_comma() {
        let list = parse_selector_list("h1, h2, .foo");
        assert_eq!(
            list,
            vec![
                Selector::Tag("h1".to_string()),
                Selector::Tag("h2".to_string()),
                Selector::Class("foo".to_string()),
            ]
        );
    }

    /// parse_selector_list_empty
    #[test]
    fn parse_selector_list_empty() {
        let list = parse_selector_list("");
        assert!(list.is_empty());
    }

    /// attr_presence_specificity: `[href]` → (0, 1, 0)
    #[test]
    fn attr_presence_specificity() {
        let sel = parse_selector_group("[href]").expect("should parse");
        assert_eq!(sel.specificity(), Specificity(0, 1, 0));
    }
}

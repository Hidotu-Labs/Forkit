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

// ──────────────────────────────────────────────────────────────────────────────
// PseudoClass
// ──────────────────────────────────────────────────────────────────────────────

/// A CSS pseudo-class, e.g. `:hover`, `:nth-child(2n+1)`.
#[derive(Debug, Clone, PartialEq)]
pub enum PseudoClass {
    Hover,
    Focus,
    Active,
    Checked,
    Disabled,
    Enabled,
    Link,
    Visited,
    FirstChild,
    LastChild,
    OnlyChild,
    FirstOfType,
    LastOfType,
    OnlyOfType,
    /// `(A, B)`: matches when `A*n + B == position` (1-based, n ≥ 0).
    NthChild(i32, i32),
    NthLastChild(i32, i32),
    NthOfType(i32, i32),
    Empty,
    /// `:not(<simple-selector>)`
    Not(Box<SimpleSelector>),
    /// Any pseudo-class the engine does not recognise.
    Unknown(String),
}

// ──────────────────────────────────────────────────────────────────────────────
// SimpleSelector
// ──────────────────────────────────────────────────────────────────────────────

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
    /// `[class~="foo"]` — attribute value is a whitespace-separated list containing word.
    AttrContainsWord(String, String),
    /// `[href^="https"]` — attribute value starts with prefix.
    AttrStartsWith(String, String),
    /// `[href$=".pdf"]` — attribute value ends with suffix.
    AttrEndsWith(String, String),
    /// `[href*="example"]` — attribute value contains substring.
    AttrContains(String, String),
    /// A CSS pseudo-class, e.g. `:hover`, `:nth-child(2n+1)`.
    Pseudo(PseudoClass),
}

// ──────────────────────────────────────────────────────────────────────────────
// Selector
// ──────────────────────────────────────────────────────────────────────────────

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
    /// General sibling combinator (`A ~ B`): `B` follows `A` somewhere in the same parent.
    GeneralSibling(Box<Selector>, Box<Selector>),
}

// ──────────────────────────────────────────────────────────────────────────────
// Specificity
// ──────────────────────────────────────────────────────────────────────────────

/// CSS specificity, represented as `(id, class, tag)` counts.
///
/// Higher values take precedence; compare left-to-right (id first).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Specificity(pub u32, pub u32, pub u32);

// ──────────────────────────────────────────────────────────────────────────────
// StyleSheet / Rule
// ──────────────────────────────────────────────────────────────────────────────

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

// ──────────────────────────────────────────────────────────────────────────────
// Selector::specificity
// ──────────────────────────────────────────────────────────────────────────────

impl Selector {
    /// Compute the CSS specificity of this selector as `(id_count, class_count, tag_count)`.
    ///
    /// Rules:
    /// - Each `#id` contributes `(1, 0, 0)`.
    /// - Each `.class`, attribute presence/equality, or pseudo-class contributes `(0, 1, 0)`.
    /// - Each tag name contributes `(0, 0, 1)`.
    /// - `*` (universal) contributes nothing.
    /// - Combinators sum the specificity of both sides.
    /// - `Compound` sums the specificity of every `SimpleSelector` inside it.
    pub fn specificity(&self) -> Specificity {
        match self {
            Selector::Tag(_)       => Specificity(0, 0, 1),
            Selector::Class(_)     => Specificity(0, 1, 0),
            Selector::Id(_)        => Specificity(1, 0, 0),
            Selector::Universal    => Specificity(0, 0, 0),
            Selector::Compound(parts) => {
                let mut ids = 0u32;
                let mut classes = 0u32;
                let mut tags = 0u32;
                for ss in parts {
                    match ss {
                        SimpleSelector::Id(_) => ids += 1,
                        SimpleSelector::Class(_)
                        | SimpleSelector::AttrPresence(_)
                        | SimpleSelector::AttrEquality(_, _)
                        | SimpleSelector::AttrContainsWord(_, _)
                        | SimpleSelector::AttrStartsWith(_, _)
                        | SimpleSelector::AttrEndsWith(_, _)
                        | SimpleSelector::AttrContains(_, _)
                        | SimpleSelector::Pseudo(_) => classes += 1,
                        SimpleSelector::Tag(_) => tags += 1,
                        SimpleSelector::Universal => {}
                    }
                }
                Specificity(ids, classes, tags)
            }
            Selector::Descendant(a, b)
            | Selector::Child(a, b)
            | Selector::AdjacentSibling(a, b)
            | Selector::GeneralSibling(a, b) => {
                let Specificity(ai, ac, at) = a.specificity();
                let Specificity(bi, bc, bt) = b.specificity();
                Specificity(ai + bi, ac + bc, at + bt)
            }
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Public parse helpers
// ──────────────────────────────────────────────────────────────────────────────

/// Parse a comma-separated selector list string into a `Vec<Selector>`.
///
/// Each comma-separated segment is parsed as a selector group.
/// Invalid or empty segments are silently skipped.
pub fn parse_selector_list(s: &str) -> Vec<Selector> {
    s.split(',')
        .map(|seg| seg.trim())
        .filter(|seg| !seg.is_empty())
        .filter_map(parse_selector_group)
        .collect()
}

/// Parse a single selector group string (no commas) into a `Selector`.
///
/// Handles descendant (space), child (`>`), adjacent sibling (`+`), and
/// general sibling (`~`) combinators, left-to-right.
/// Returns `None` for empty or completely unparseable input.
pub fn parse_selector_group(s: &str) -> Option<Selector> {
    parse_selector(s)
}

/// Parse all simple selector components out of a compound selector token
/// (a token that contains no combinators), e.g. `div.foo#bar[href]`.
///
/// Returns a `Vec<SimpleSelector>` — one entry per component.
/// An empty vec is returned for empty input.
pub fn parse_simple_selector(token: &str) -> Vec<SimpleSelector> {
    split_simple_selectors(token)
}

/// Parse a pseudo-class by name (lowercase) and optional argument string.
///
/// Examples:
/// - `parse_pseudo_class("hover", None)` → `PseudoClass::Hover`
/// - `parse_pseudo_class("nth-child", Some("2n+1"))` → `PseudoClass::NthChild(2, 1)`
/// - `parse_pseudo_class("not", Some(".foo"))` → `PseudoClass::Not(Box::new(SimpleSelector::Class("foo")))`
pub fn parse_pseudo_class(name: &str, arg: Option<&str>) -> PseudoClass {
    match name {
        "hover"          => PseudoClass::Hover,
        "focus"          => PseudoClass::Focus,
        "active"         => PseudoClass::Active,
        "checked"        => PseudoClass::Checked,
        "disabled"       => PseudoClass::Disabled,
        "enabled"        => PseudoClass::Enabled,
        "link"           => PseudoClass::Link,
        "visited"        => PseudoClass::Visited,
        "first-child"    => PseudoClass::FirstChild,
        "last-child"     => PseudoClass::LastChild,
        "only-child"     => PseudoClass::OnlyChild,
        "first-of-type"  => PseudoClass::FirstOfType,
        "last-of-type"   => PseudoClass::LastOfType,
        "only-of-type"   => PseudoClass::OnlyOfType,
        "empty"          => PseudoClass::Empty,
        "nth-child"      => {
            let (a, b) = parse_nth(arg.unwrap_or("0"));
            PseudoClass::NthChild(a, b)
        }
        "nth-last-child" => {
            let (a, b) = parse_nth(arg.unwrap_or("0"));
            PseudoClass::NthLastChild(a, b)
        }
        "nth-of-type"    => {
            let (a, b) = parse_nth(arg.unwrap_or("0"));
            PseudoClass::NthOfType(a, b)
        }
        "not" => {
            let inner_str = arg.unwrap_or("").trim();
            // Parse the argument as a single simple-selector component
            let parts = split_simple_selectors(inner_str);
            let ss = parts.into_iter().next().unwrap_or(SimpleSelector::Universal);
            PseudoClass::Not(Box::new(ss))
        }
        other => PseudoClass::Unknown(other.to_string()),
    }
}

/// Parse an An+B expression from a CSS `:nth-child(…)` argument.
///
/// Supported forms:
/// - `"odd"`  → `(2, 1)`
/// - `"even"` → `(2, 0)`
/// - `"3"`    → `(0, 3)`   — pure integer
/// - `"2n"`   → `(2, 0)`
/// - `"2n+1"` → `(2, 1)`
/// - `"3n-2"` → `(3, -2)`
/// - `"n"`    → `(1, 0)`
/// - `"-n+3"` → `(-1, 3)`
pub fn parse_nth(s: &str) -> (i32, i32) {
    let s = s.trim().to_ascii_lowercase();
    let s = s.as_str();

    match s {
        "odd"  => return (2, 1),
        "even" => return (2, 0),
        _      => {}
    }

    // Does the string contain 'n'?
    if let Some(n_pos) = s.find('n') {
        // Part before 'n' is A
        let a_str = s[..n_pos].trim();
        let a: i32 = if a_str.is_empty() || a_str == "+" {
            1
        } else if a_str == "-" {
            -1
        } else {
            a_str.parse().unwrap_or(1)
        };

        // Part after 'n' is ±B
        let after = s[n_pos + 1..].trim();
        let b: i32 = if after.is_empty() {
            0
        } else {
            // after is like "+3" or "-2" or "3"
            after.parse().unwrap_or(0)
        };

        (a, b)
    } else {
        // Pure integer — treat as (0, B)
        let b: i32 = s.parse().unwrap_or(0);
        (0, b)
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// StyleSheet::parse
// ──────────────────────────────────────────────────────────────────────────────

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
            // We need to be careful about pseudo-selectors with parens: `:not(.foo)`
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

            // Parse comma-separated selectors, splitting carefully to avoid
            // splitting inside :not(...) or other functional pseudo-classes.
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

// ──────────────────────────────────────────────────────────────────────────────
// Internal helpers
// ──────────────────────────────────────────────────────────────────────────────

/// Split a selector list on commas, but NOT on commas inside parentheses
/// (e.g. `:not(.a, .b)` should remain intact as one unit — though we only
/// support one argument per :not() currently, this keeps the parser robust).
fn split_selector_list(s: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    for c in s.chars() {
        match c {
            '(' => { depth += 1; current.push(c); }
            ')' => { if depth > 0 { depth -= 1; } current.push(c); }
            ',' if depth == 0 => {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() { result.push(trimmed); }
                current.clear();
            }
            _ => { current.push(c); }
        }
    }
    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() { result.push(trimmed); }
    result
}

/// Remove `/* … */` block comments from the source string.
fn strip_comments(src: &str) -> String {
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

/// Skip whitespace characters in `chars` starting at `pos`.
fn skip_whitespace(chars: &[char], pos: &mut usize) {
    while *pos < chars.len() && chars[*pos].is_whitespace() {
        *pos += 1;
    }
}

/// Skip an at-rule starting at `@`.
fn skip_at_rule(chars: &[char], pos: &mut usize) {
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
fn skip_to_closing_brace(chars: &[char], pos: &mut usize) {
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

/// Parse a single selector string (no commas) into a `Selector`.
///
/// Handles combinators: descendant (space), `>` child, `+` adjacent sibling, `~` general sibling.
/// Returns `None` if the string is empty or completely unparseable.
fn parse_selector(s: &str) -> Option<Selector> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    #[derive(Debug)]
    enum CombinatorKind {
        Descendant,
        Child,
        AdjacentSibling,
        GeneralSibling,
    }

    let chars: Vec<char> = s.chars().collect();
    let len = chars.len();
    let mut i = 0;

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

            // Pseudo-class / pseudo-element — grab `:name` or `:name(args)`
            ':' => {
                current_token.push(':');
                i += 1;
                // Skip a second `:` for pseudo-elements (::before)
                if i < len && chars[i] == ':' {
                    current_token.push(':');
                    i += 1;
                }
                // Collect the pseudo name
                while i < len && (chars[i].is_alphanumeric() || chars[i] == '-' || chars[i] == '_') {
                    current_token.push(chars[i]);
                    i += 1;
                }
                // Collect argument in parens, if present
                if i < len && chars[i] == '(' {
                    current_token.push('(');
                    i += 1;
                    let mut depth = 1usize;
                    while i < len && depth > 0 {
                        match chars[i] {
                            '(' => { depth += 1; current_token.push(chars[i]); }
                            ')' => {
                                depth -= 1;
                                if depth == 0 {
                                    current_token.push(')');
                                } else {
                                    current_token.push(chars[i]);
                                }
                            }
                            c => { current_token.push(c); }
                        }
                        i += 1;
                    }
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
            '~' => {
                if !current_token.trim().is_empty() {
                    parts.push((pending_combinator.take(), current_token.trim().to_string()));
                    current_token.clear();
                }
                pending_combinator = Some(CombinatorKind::GeneralSibling);
                saw_whitespace = false;
                i += 1;
            }
            c if c.is_whitespace() => {
                saw_whitespace = true;
                i += 1;
            }
            c => {
                // If we had whitespace and no explicit combinator, flush as Descendant
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
            (Some(left), Some(CombinatorKind::GeneralSibling)) => {
                Selector::GeneralSibling(Box::new(left), Box::new(right))
            }
            (Some(left), None) => {
                Selector::Descendant(Box::new(left), Box::new(right))
            }
        });
    }

    result
}

/// Parse a compound selector token (no combinators) like `div.foo#bar[type="text"]:hover`
/// into a `Selector`.
///
/// - A single simple component is unwrapped to `Tag`, `Class`, `Id`, or `Universal`.
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

    if components.len() == 1 {
        return match &components[0] {
            SimpleSelector::Tag(t)     => Some(Selector::Tag(t.clone())),
            SimpleSelector::Class(c)   => Some(Selector::Class(c.clone())),
            SimpleSelector::Id(id)     => Some(Selector::Id(id.clone())),
            SimpleSelector::Universal  => Some(Selector::Universal),
            _                          => Some(Selector::Compound(components)),
        };
    }

    Some(Selector::Compound(components))
}

/// Split a compound-selector token like `div.foo#bar[href]:hover` into
/// individual `SimpleSelector`s.
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
                parse_attr_selector(&attr_content, &mut result);
            }
            ':' => {
                i += 1; // skip first `:`
                // Pseudo-element (::before, ::after) — skip silently
                if i < len && chars[i] == ':' {
                    i += 1;
                    // skip the pseudo-element name
                    take_ident(&chars, &mut i);
                    // skip any argument
                    if i < len && chars[i] == '(' {
                        i += 1;
                        let mut depth = 1usize;
                        while i < len && depth > 0 {
                            if chars[i] == '(' { depth += 1; }
                            else if chars[i] == ')' { depth -= 1; }
                            i += 1;
                        }
                    }
                    continue;
                }
                // Pseudo-class
                let name = take_ident_hyphenated(&chars, &mut i);
                if name.is_empty() {
                    // malformed, skip
                    continue;
                }
                // Check for argument
                let arg = if i < len && chars[i] == '(' {
                    i += 1; // skip `(`
                    let arg_str = take_pseudo_arg(&chars, &mut i);
                    // take_pseudo_arg already consumed the closing `)`
                    Some(arg_str)
                } else {
                    None
                };
                let pc = parse_pseudo_class(&name, arg.as_deref());
                result.push(SimpleSelector::Pseudo(pc));
            }
            _ => {
                // Tag name
                let name = take_ident(&chars, &mut i);
                if !name.is_empty() {
                    result.push(SimpleSelector::Tag(name));
                } else {
                    i += 1; // skip unrecognised char
                }
            }
        }
    }

    result
}

/// Parse an attribute selector content string (inside `[…]`) and push the
/// appropriate `SimpleSelector` variant onto `result`.
fn parse_attr_selector(content: &str, result: &mut Vec<SimpleSelector>) {
    // Check for extended operators: ~=, ^=, $=, *=, then plain =
    if let Some(pos) = find_attr_op(content) {
        let (op_start, op_len) = pos;
        let attr = content[..op_start].trim().to_string();
        let val  = content[op_start + op_len..]
            .trim()
            .trim_matches('"')
            .trim_matches('\'')
            .to_string();
        let op = &content[op_start..op_start + op_len];
        match op {
            "~=" => result.push(SimpleSelector::AttrContainsWord(attr, val)),
            "^=" => result.push(SimpleSelector::AttrStartsWith(attr, val)),
            "$=" => result.push(SimpleSelector::AttrEndsWith(attr, val)),
            "*=" => result.push(SimpleSelector::AttrContains(attr, val)),
            "="  => result.push(SimpleSelector::AttrEquality(attr, val)),
            _    => result.push(SimpleSelector::AttrEquality(attr, val)),
        }
    } else {
        let attr = content.trim().to_string();
        if !attr.is_empty() {
            result.push(SimpleSelector::AttrPresence(attr));
        }
    }
}

/// Find an attribute operator in the content string.
/// Returns `Some((start_byte_pos, op_len))` for the first operator found,
/// in priority order: `~=`, `^=`, `$=`, `*=`, `=`.
fn find_attr_op(content: &str) -> Option<(usize, usize)> {
    let bytes = content.as_bytes();
    let len = bytes.len();
    for i in 0..len {
        if i + 1 < len {
            match (bytes[i], bytes[i + 1]) {
                (b'~', b'=') | (b'^', b'=') | (b'$', b'=') | (b'*', b'=') => {
                    return Some((i, 2));
                }
                _ => {}
            }
        }
        if bytes[i] == b'=' {
            // Make sure this isn't part of a two-char op we already checked
            return Some((i, 1));
        }
    }
    None
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

/// Like `take_ident`, but also accepts hyphen (same as `take_ident` — kept for clarity).
fn take_ident_hyphenated(chars: &[char], pos: &mut usize) -> String {
    take_ident(chars, pos)
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

/// Collect the content of a pseudo-class argument `(…)`, handling nested parens.
/// Assumes the opening `(` was already consumed.  Stops when the matching `)`
/// is found.  On return, `*pos` points to the character AFTER the closing `)`.
fn take_pseudo_arg(chars: &[char], pos: &mut usize) -> String {
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

/// Parse the text between `{` and `}` into `(property, value)` pairs.
/// Correctly handles semicolons inside url(...) and quoted strings.
fn parse_declarations(block: &str, _selector: &str) -> Vec<(String, String)> {
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

        // Find the first `:` not inside parens/quotes
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
                    continue; // silent skip — pseudo-selectors produce these
                }
                result.push((property, value));
            }
            None => {
                // No colon — skip silently
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

    /// syntax_error_recovery
    #[test]
    fn syntax_error_recovery() {
        let css = "div { no-colon-here } p { color: blue; }";
        let ss = StyleSheet::parse(css);
        assert!(!ss.rules.is_empty(), "expected at least one rule");
        let p_rule = ss.rules.iter().find(|r| {
            r.selectors.iter().any(|s| matches!(s, Selector::Tag(t) if t == "p"))
        });
        assert!(p_rule.is_some(), "valid `p` rule should be present after error recovery");
        let p_rule = p_rule.unwrap();
        assert_eq!(p_rule.declarations, vec![("color".to_string(), "blue".to_string())]);
    }

    /// import_ignored
    #[test]
    fn import_ignored() {
        let ss = StyleSheet::parse("@import \"style.css\"; p { color: red; }");
        assert_eq!(ss.rules.len(), 1);
        let rule = &ss.rules[0];
        assert_eq!(rule.selectors, vec![Selector::Tag("p".to_string())]);
        assert_eq!(rule.declarations[0], ("color".to_string(), "red".to_string()));
    }

    /// block_comment_stripped
    #[test]
    fn block_comment_stripped() {
        let ss = StyleSheet::parse("/* this is a comment */ p { color: green; }");
        assert_eq!(ss.rules.len(), 1);
        assert_eq!(ss.rules[0].declarations[0], ("color".to_string(), "green".to_string()));
    }

    /// multiple_declarations
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

    /// import_url_ignored
    #[test]
    fn import_url_ignored() {
        let ss = StyleSheet::parse("@import url(reset.css); h1 { font-size: 2em; }");
        assert_eq!(ss.rules.len(), 1);
        assert_eq!(ss.rules[0].selectors, vec![Selector::Tag("h1".to_string())]);
    }

    // ── Selector parsing tests ──────────────────────────────────────────────

    #[test]
    fn parse_selector_tag() {
        let sel = parse_selector("div");
        assert_eq!(sel, Some(Selector::Tag("div".to_string())));
    }

    #[test]
    fn parse_selector_class() {
        let sel = parse_selector(".foo");
        assert_eq!(sel, Some(Selector::Class("foo".to_string())));
    }

    #[test]
    fn parse_selector_id() {
        let sel = parse_selector("#bar");
        assert_eq!(sel, Some(Selector::Id("bar".to_string())));
    }

    #[test]
    fn parse_selector_universal() {
        let sel = parse_selector("*");
        assert_eq!(sel, Some(Selector::Universal));
    }

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

    #[test]
    fn parse_selector_child() {
        let sel = parse_selector("ul > li");
        assert_eq!(
            sel,
            Some(Selector::Child(
                Box::new(Selector::Tag("ul".into())),
                Box::new(Selector::Tag("li".into())),
            ))
        );
    }

    #[test]
    fn parse_selector_adjacent() {
        let sel = parse_selector("h1 + p");
        assert_eq!(
            sel,
            Some(Selector::AdjacentSibling(
                Box::new(Selector::Tag("h1".into())),
                Box::new(Selector::Tag("p".into())),
            ))
        );
    }

    #[test]
    fn parse_selector_general_sibling() {
        let sel = parse_selector("h1 ~ p");
        assert_eq!(
            sel,
            Some(Selector::GeneralSibling(
                Box::new(Selector::Tag("h1".into())),
                Box::new(Selector::Tag("p".into())),
            ))
        );
    }

    #[test]
    fn parse_selector_attr_presence() {
        let sel = parse_selector("[href]");
        assert_eq!(
            sel,
            Some(Selector::Compound(vec![SimpleSelector::AttrPresence("href".to_string())]))
        );
    }

    #[test]
    fn parse_selector_attr_equality() {
        let sel = parse_selector("[type=\"text\"]");
        assert_eq!(
            sel,
            Some(Selector::Compound(vec![
                SimpleSelector::AttrEquality("type".to_string(), "text".to_string()),
            ]))
        );
    }

    #[test]
    fn parse_selector_attr_starts_with() {
        let sel = parse_selector("[href^=\"https\"]");
        assert_eq!(
            sel,
            Some(Selector::Compound(vec![
                SimpleSelector::AttrStartsWith("href".to_string(), "https".to_string()),
            ]))
        );
    }

    #[test]
    fn parse_selector_attr_ends_with() {
        let sel = parse_selector("[href$=\".pdf\"]");
        assert_eq!(
            sel,
            Some(Selector::Compound(vec![
                SimpleSelector::AttrEndsWith("href".to_string(), ".pdf".to_string()),
            ]))
        );
    }

    #[test]
    fn parse_selector_attr_contains() {
        let sel = parse_selector("[href*=\"example\"]");
        assert_eq!(
            sel,
            Some(Selector::Compound(vec![
                SimpleSelector::AttrContains("href".to_string(), "example".to_string()),
            ]))
        );
    }

    #[test]
    fn parse_selector_attr_contains_word() {
        let sel = parse_selector("[class~=\"foo\"]");
        assert_eq!(
            sel,
            Some(Selector::Compound(vec![
                SimpleSelector::AttrContainsWord("class".to_string(), "foo".to_string()),
            ]))
        );
    }

    #[test]
    fn parse_selector_pseudo_hover() {
        let sel = parse_selector("a:hover");
        assert_eq!(
            sel,
            Some(Selector::Compound(vec![
                SimpleSelector::Tag("a".to_string()),
                SimpleSelector::Pseudo(PseudoClass::Hover),
            ]))
        );
    }

    #[test]
    fn parse_selector_pseudo_nth_child() {
        let sel = parse_selector("li:nth-child(2n+1)");
        assert_eq!(
            sel,
            Some(Selector::Compound(vec![
                SimpleSelector::Tag("li".to_string()),
                SimpleSelector::Pseudo(PseudoClass::NthChild(2, 1)),
            ]))
        );
    }

    #[test]
    fn parse_selector_pseudo_not() {
        let sel = parse_selector("p:not(.foo)");
        assert_eq!(
            sel,
            Some(Selector::Compound(vec![
                SimpleSelector::Tag("p".to_string()),
                SimpleSelector::Pseudo(PseudoClass::Not(
                    Box::new(SimpleSelector::Class("foo".to_string()))
                )),
            ]))
        );
    }

    #[test]
    fn parse_pseudo_element_skipped() {
        // ::before pseudo-element should be silently skipped, leaving just the tag
        let components = split_simple_selectors("p::before");
        // Should produce only Tag("p"), the ::before is skipped
        assert_eq!(components, vec![SimpleSelector::Tag("p".to_string())]);
    }

    // ── parse_nth tests ─────────────────────────────────────────────────────

    #[test]
    fn parse_nth_odd() {
        assert_eq!(parse_nth("odd"), (2, 1));
    }

    #[test]
    fn parse_nth_even() {
        assert_eq!(parse_nth("even"), (2, 0));
    }

    #[test]
    fn parse_nth_integer() {
        assert_eq!(parse_nth("3"), (0, 3));
    }

    #[test]
    fn parse_nth_an_plus_b() {
        assert_eq!(parse_nth("2n+1"), (2, 1));
    }

    #[test]
    fn parse_nth_an_minus_b() {
        assert_eq!(parse_nth("3n-2"), (3, -2));
    }

    #[test]
    fn parse_nth_n_only() {
        assert_eq!(parse_nth("n"), (1, 0));
    }

    #[test]
    fn parse_nth_neg_n_plus_b() {
        assert_eq!(parse_nth("-n+3"), (-1, 3));
    }

    // ── Specificity tests ───────────────────────────────────────────────────

    #[test]
    fn specificity_ordering() {
        let tag   = Specificity(0, 0, 1);
        let class = Specificity(0, 1, 0);
        let id    = Specificity(1, 0, 0);
        assert!(tag < class);
        assert!(class < id);
        assert!(tag < id);
    }

    #[test]
    fn specificity_equal() {
        assert_eq!(Specificity(1, 2, 3), Specificity(1, 2, 3));
    }

    #[test]
    fn specificity_compound_div_foo_bar() {
        let sel = parse_selector_group("div.foo#bar").expect("should parse");
        assert_eq!(sel.specificity(), Specificity(1, 1, 1));
    }

    #[test]
    fn specificity_id_only() {
        let sel = parse_selector_group("#main").expect("should parse");
        assert_eq!(sel.specificity(), Specificity(1, 0, 0));
    }

    #[test]
    fn specificity_class_only() {
        let sel = parse_selector_group(".btn").expect("should parse");
        assert_eq!(sel.specificity(), Specificity(0, 1, 0));
    }

    #[test]
    fn specificity_tag_only() {
        let sel = parse_selector_group("p").expect("should parse");
        assert_eq!(sel.specificity(), Specificity(0, 0, 1));
    }

    #[test]
    fn specificity_universal() {
        let sel = parse_selector_group("*").expect("should parse");
        assert_eq!(sel.specificity(), Specificity(0, 0, 0));
    }

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

    #[test]
    fn child_combinator_specificity() {
        let sel = parse_selector_group("a > b").expect("should parse");
        assert_eq!(sel.specificity(), Specificity(0, 0, 2));
    }

    #[test]
    fn parse_simple_selector_attr_presence() {
        let parts = parse_simple_selector("[href]");
        assert_eq!(parts, vec![SimpleSelector::AttrPresence("href".to_string())]);
    }

    #[test]
    fn parse_simple_selector_attr_equality() {
        let parts = parse_simple_selector("[type=\"text\"]");
        assert_eq!(
            parts,
            vec![SimpleSelector::AttrEquality("type".to_string(), "text".to_string())]
        );
    }

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

    #[test]
    fn parse_selector_list_empty() {
        let list = parse_selector_list("");
        assert!(list.is_empty());
    }

    #[test]
    fn attr_presence_specificity() {
        let sel = parse_selector_group("[href]").expect("should parse");
        assert_eq!(sel.specificity(), Specificity(0, 1, 0));
    }

    #[test]
    fn pseudo_class_specificity() {
        // a:hover → (0, 1, 1)
        let sel = parse_selector_group("a:hover").expect("should parse");
        assert_eq!(sel.specificity(), Specificity(0, 1, 1));
    }

    #[test]
    fn general_sibling_specificity() {
        // h1 ~ p → (0, 0, 2)
        let sel = parse_selector_group("h1 ~ p").expect("should parse");
        assert_eq!(sel.specificity(), Specificity(0, 0, 2));
    }
}

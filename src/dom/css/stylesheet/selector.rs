use super::utils::*;

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
    /// `:root` — matches the document root element (`<html>`).
    Root,
    /// `:not(<simple-selector>)`
    Not(Box<SimpleSelector>),
    /// Any pseudo-class the engine does not recognise.
    Unknown(String),
}

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

/// CSS specificity, represented as `(id, class, tag)` counts.
///
/// Higher values take precedence; compare left-to-right (id first).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Specificity(pub u32, pub u32, pub u32);

impl Selector {
    /// Compute the CSS specificity of this selector as `(id_count, class_count, tag_count)`.
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

/// Parse a comma-separated selector list string into a `Vec<Selector>`.
pub fn parse_selector_list(s: &str) -> Vec<Selector> {
    split_selector_list(s)
        .into_iter()
        .filter_map(|seg| parse_selector(&seg))
        .collect()
}

/// Parse a single selector group string (no commas) into a `Selector`.
pub fn parse_selector_group(s: &str) -> Option<Selector> {
    parse_selector(s)
}

/// Split a selector list on commas, but NOT on commas inside parentheses.
pub fn split_selector_list(s: &str) -> Vec<String> {
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

/// Parse all simple selector components out of a compound selector token.
pub fn parse_simple_selector(token: &str) -> Vec<SimpleSelector> {
    split_simple_selectors(token)
}

/// Parse a pseudo-class by name (lowercase) and optional argument string.
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
            let parts = split_simple_selectors(inner_str);
            let ss = parts.into_iter().next().unwrap_or(SimpleSelector::Universal);
            PseudoClass::Not(Box::new(ss))
        }
        "root" => PseudoClass::Root,
        other => PseudoClass::Unknown(other.to_string()),
    }
}

/// Parse an An+B expression from a CSS `:nth-child(…)` argument.
pub fn parse_nth(s: &str) -> (i32, i32) {
    let s = s.trim().to_ascii_lowercase();
    let s = s.as_str();

    match s {
        "odd"  => return (2, 1),
        "even" => return (2, 0),
        _      => {}
    }

    if let Some(n_pos) = s.find('n') {
        let a_str = s[..n_pos].trim();
        let a: i32 = if a_str.is_empty() || a_str == "+" {
            1
        } else if a_str == "-" {
            -1
        } else {
            a_str.parse().unwrap_or(1)
        };

        let after = s[n_pos + 1..].trim();
        let b: i32 = if after.is_empty() {
            0
        } else {
            after.parse().unwrap_or(0)
        };

        (a, b)
    } else {
        let b: i32 = s.parse().unwrap_or(0);
        (0, b)
    }
}

/// Parse a single selector string (no commas) into a `Selector`.
pub fn parse_selector(s: &str) -> Option<Selector> {
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
            ':' => {
                current_token.push(':');
                i += 1;
                if i < len && chars[i] == ':' {
                    current_token.push(':');
                    i += 1;
                }
                while i < len && (chars[i].is_alphanumeric() || chars[i] == '-' || chars[i] == '_') {
                    current_token.push(chars[i]);
                    i += 1;
                }
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

    if !current_token.trim().is_empty() {
        parts.push((pending_combinator.take(), current_token.trim().to_string()));
    }

    if parts.is_empty() {
        return None;
    }

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

/// Parse a compound selector token (no combinators) into a `Selector`.
pub fn parse_compound_selector(token: &str) -> Option<Selector> {
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

/// Split a compound-selector token into individual `SimpleSelector`s.
pub fn split_simple_selectors(token: &str) -> Vec<SimpleSelector> {
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
                i += 1;
                let attr_content = take_until(&chars, &mut i, ']');
                i += 1;
                parse_attr_selector(&attr_content, &mut result);
            }
            ':' => {
                i += 1;
                if i < len && chars[i] == ':' {
                    i += 1;
                    take_ident(&chars, &mut i);
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
                let name = take_ident_hyphenated(&chars, &mut i);
                if name.is_empty() {
                    continue;
                }
                let arg = if i < len && chars[i] == '(' {
                    i += 1;
                    let arg_str = take_pseudo_arg(&chars, &mut i);
                    Some(arg_str)
                } else {
                    None
                };
                let pc = parse_pseudo_class(&name, arg.as_deref());
                result.push(SimpleSelector::Pseudo(pc));
            }
            _ => {
                let name = take_ident(&chars, &mut i);
                if !name.is_empty() {
                    result.push(SimpleSelector::Tag(name));
                } else {
                    i += 1;
                }
            }
        }
    }

    result
}

/// Parse an attribute selector content string and push onto `result`.
fn parse_attr_selector(content: &str, result: &mut Vec<SimpleSelector>) {
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
            return Some((i, 1));
        }
    }
    None
}

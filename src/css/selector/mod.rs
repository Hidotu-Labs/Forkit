use crate::html5::node::Element;
use crate::html5::parser::get_attr;

#[derive(Debug, Clone, PartialEq)]
pub enum Combinator {
    Descendant,       // ' '
    Child,            // '>'
    NextSibling,      // '+'
    SubsequentSibling,// '~'
}

#[derive(Debug, Clone, PartialEq)]
pub enum AttrMatcher {
    Exists,
    Exact(String),
    ExactCaseInsensitive(String),
    Includes(String), // ~=
    Prefix(String),   // ^=
    Suffix(String),   // $=
    Substring(String),// *=
}

#[derive(Debug, Clone, PartialEq)]
pub struct AttributeSelector {
    pub name: String,
    pub matcher: AttrMatcher,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PseudoClass {
    Is(Vec<ComplexSelector>),
    Where(Vec<ComplexSelector>),
    Not(Vec<ComplexSelector>),
    Root,
    Empty,
    Link,
    Visited,
    AnyLink,
    FirstChild,
    Hover,
    Active,
    Focus,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CompoundSelector {
    pub tag: Option<String>,
    pub id: Option<String>,
    pub classes: Vec<String>,
    pub attributes: Vec<AttributeSelector>,
    pub pseudos: Vec<PseudoClass>,
    pub pseudo_element: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ComplexSelector {
    // (combinator to next compound, compound)
    // The selector is a list of compounds with combinators between them:
    // e.g. "div > p.alert" -> [ (Compound(div), Child), (Compound(p.alert), None) ]
    pub units: Vec<(CompoundSelector, Option<Combinator>)>,
}

/// Split top-level commas respecting parentheses and brackets.
/// e.g. "h1, :is(h2, h3)" -> ["h1", ":is(h2, h3)"]
pub fn split_selector_list(s: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut paren_depth = 0;
    let mut bracket_depth = 0;
    let mut in_quote = None;

    for c in s.chars() {
        if let Some(q) = in_quote {
            if c == q {
                in_quote = None;
            }
            current.push(c);
            continue;
        }

        match c {
            '"' | '\'' => {
                in_quote = Some(c);
                current.push(c);
            }
            '(' => {
                paren_depth += 1;
                current.push(c);
            }
            ')' => {
                if paren_depth > 0 {
                    paren_depth -= 1;
                }
                current.push(c);
            }
            '[' => {
                bracket_depth += 1;
                current.push(c);
            }
            ']' => {
                if bracket_depth > 0 {
                    bracket_depth -= 1;
                }
                current.push(c);
            }
            ',' if paren_depth == 0 && bracket_depth == 0 => {
                let trimmed = current.trim();
                if !trimmed.is_empty() {
                    result.push(trimmed.to_string());
                }
                current.clear();
            }
            _ => {
                current.push(c);
            }
        }
    }

    let trimmed = current.trim();
    if !trimmed.is_empty() {
        result.push(trimmed.to_string());
    }

    result
}

impl ComplexSelector {
    pub fn parse(input: &str) -> Option<ComplexSelector> {
        let input = input.trim();
        if input.is_empty() {
            return None;
        }

        // Tokenize into compound strings and combinators
        let mut units = Vec::new();
        let chars: Vec<char> = input.chars().collect();
        let len = chars.len();
        let mut i = 0;

        let mut current_compound = String::new();
        let mut paren_depth = 0;
        let mut bracket_depth = 0;

        while i < len {
            let c = chars[i];
            match c {
                '(' => {
                    paren_depth += 1;
                    current_compound.push(c);
                    i += 1;
                }
                ')' => {
                    if paren_depth > 0 {
                        paren_depth -= 1;
                    }
                    current_compound.push(c);
                    i += 1;
                }
                '[' => {
                    bracket_depth += 1;
                    current_compound.push(c);
                    i += 1;
                }
                ']' => {
                    if bracket_depth > 0 {
                        bracket_depth -= 1;
                    }
                    current_compound.push(c);
                    i += 1;
                }
                '>' | '+' | '~' if paren_depth == 0 && bracket_depth == 0 => {
                    let comb = match c {
                        '>' => Combinator::Child,
                        '+' => Combinator::NextSibling,
                        '~' => Combinator::SubsequentSibling,
                        _ => unreachable!(),
                    };
                    let trimmed = current_compound.trim();
                    if !trimmed.is_empty() {
                        let compound = CompoundSelector::parse(trimmed)?;
                        units.push((compound, Some(comb)));
                        current_compound.clear();
                    } else if let Some(last) = units.last_mut() {
                        // e.g. "div >" where space preceding '>' already set a combinator
                        last.1 = Some(comb);
                    }
                    i += 1;
                }
                ' ' | '\t' | '\n' | '\r' if paren_depth == 0 && bracket_depth == 0 => {
                    // Check if this whitespace is a descendant combinator
                    let trimmed = current_compound.trim();
                    if !trimmed.is_empty() {
                        let compound = CompoundSelector::parse(trimmed)?;
                        // peek ahead: if next non-whitespace is '>', '+', '~', don't add Descendant yet
                        let mut next_i = i + 1;
                        while next_i < len && chars[next_i].is_whitespace() {
                            next_i += 1;
                        }
                        if next_i < len && (chars[next_i] == '>' || chars[next_i] == '+' || chars[next_i] == '~') {
                            units.push((compound, None));
                        } else {
                            units.push((compound, Some(Combinator::Descendant)));
                        }
                        current_compound.clear();
                    }
                    i += 1;
                }
                _ => {
                    current_compound.push(c);
                    i += 1;
                }
            }
        }

        let trimmed = current_compound.trim();
        if !trimmed.is_empty() {
            let compound = CompoundSelector::parse(trimmed)?;
            units.push((compound, None));
        }

        if units.is_empty() {
            None
        } else {
            Some(ComplexSelector { units })
        }
    }

    /// Evaluates if `chain` (ancestors + current element at chain.last()) matches this complex selector.
    pub fn matches(&self, chain: &[&Element]) -> bool {
        self.matches_with_siblings(chain, &[])
    }

    /// Evaluates with preceding element siblings of the current element.
    pub fn matches_with_siblings(&self, chain: &[&Element], prev_siblings: &[&Element]) -> bool {
        if chain.is_empty() || self.units.is_empty() {
            return false;
        }

        // Start from the rightmost compound selector (subject of selector)
        let last_unit_idx = self.units.len() - 1;
        let (ref subject_compound, _) = self.units[last_unit_idx];

        let target_el = *chain.last().unwrap();
        if !subject_compound.matches_with_siblings(target_el, chain, prev_siblings) {
            return false;
        }

        if self.units.len() == 1 {
            return true;
        }

        // Match remaining units from right to left
        self.match_ancestors(chain.len() - 1, last_unit_idx - 1, chain, prev_siblings)
    }

    fn match_ancestors(&self, current_el_idx: usize, unit_idx: usize, chain: &[&Element], prev_siblings: &[&Element]) -> bool {
        let (ref compound, ref comb) = self.units[unit_idx];
        let combinator = comb.as_ref().unwrap_or(&Combinator::Descendant);

        match combinator {
            Combinator::Child => {
                if current_el_idx == 0 {
                    return false;
                }
                let parent_idx = current_el_idx - 1;
                let subchain = &chain[..=parent_idx];
                if compound.matches(chain[parent_idx], subchain) {
                    if unit_idx == 0 {
                        return true;
                    }
                    return self.match_ancestors(parent_idx, unit_idx - 1, chain, &[]);
                }
                false
            }
            Combinator::Descendant => {
                let mut parent_idx = current_el_idx;
                while parent_idx > 0 {
                    parent_idx -= 1;
                    let subchain = &chain[..=parent_idx];
                    if compound.matches(chain[parent_idx], subchain) {
                        if unit_idx == 0 {
                            return true;
                        }
                        if self.match_ancestors(parent_idx, unit_idx - 1, chain, &[]) {
                            return true;
                        }
                    }
                }
                false
            }
            Combinator::NextSibling => {
                // Preceding immediate element sibling:
                if let Some(&prev_el) = prev_siblings.last() {
                    let mut sibling_chain = chain[..current_el_idx].to_vec();
                    sibling_chain.push(prev_el);
                    if compound.matches(prev_el, &sibling_chain) {
                        if unit_idx == 0 {
                            return true;
                        }
                        let rem_siblings = &prev_siblings[..prev_siblings.len() - 1];
                        return self.match_ancestors(current_el_idx, unit_idx - 1, chain, rem_siblings);
                    }
                }
                false
            }
            Combinator::SubsequentSibling => {
                // Any preceding element sibling:
                for (idx, &prev_el) in prev_siblings.iter().enumerate().rev() {
                    let mut sibling_chain = chain[..current_el_idx].to_vec();
                    sibling_chain.push(prev_el);
                    if compound.matches(prev_el, &sibling_chain) {
                        if unit_idx == 0 {
                            return true;
                        }
                        let rem_siblings = &prev_siblings[..idx];
                        if self.match_ancestors(current_el_idx, unit_idx - 1, chain, rem_siblings) {
                            return true;
                        }
                    }
                }
                false
            }
        }
    }
}

impl CompoundSelector {
    pub fn parse(input: &str) -> Option<CompoundSelector> {
        let input = input.trim();
        if input.is_empty() {
            return None;
        }

        let mut sel = CompoundSelector::default();
        let chars: Vec<char> = input.chars().collect();
        let len = chars.len();
        let mut i = 0;

        // Check if starts with tag or '*'
        if chars[0] != '.' && chars[0] != '#' && chars[0] != '[' && chars[0] != ':' {
            let mut tag_end = i;
            while tag_end < len && chars[tag_end] != '.' && chars[tag_end] != '#' && chars[tag_end] != '[' && chars[tag_end] != ':' {
                tag_end += 1;
            }
            let tag: String = chars[i..tag_end].iter().collect();
            if tag != "*" {
                sel.tag = Some(tag.to_ascii_lowercase());
            }
            i = tag_end;
        }

        while i < len {
            match chars[i] {
                '#' => {
                    i += 1;
                    let start = i;
                    while i < len && chars[i] != '.' && chars[i] != '#' && chars[i] != '[' && chars[i] != ':' {
                        i += 1;
                    }
                    let id: String = chars[start..i].iter().collect();
                    sel.id = Some(id);
                }
                '.' => {
                    i += 1;
                    let start = i;
                    while i < len && chars[i] != '.' && chars[i] != '#' && chars[i] != '[' && chars[i] != ':' {
                        i += 1;
                    }
                    let class: String = chars[start..i].iter().collect();
                    sel.classes.push(class);
                }
                '[' => {
                    i += 1;
                    let start = i;
                    while i < len && chars[i] != ']' {
                        i += 1;
                    }
                    let attr_content: String = chars[start..i].iter().collect();
                    if i < len && chars[i] == ']' {
                        i += 1;
                    }
                    if let Some(attr_sel) = parse_attribute_selector(&attr_content) {
                        sel.attributes.push(attr_sel);
                    }
                }
                ':' => {
                    let is_double = i + 1 < len && chars[i + 1] == ':';
                    if is_double {
                        i += 2;
                    } else {
                        i += 1;
                    }
                    let start = i;
                    let mut paren_count = 0;
                    while i < len {
                        if chars[i] == '(' {
                            paren_count += 1;
                        } else if chars[i] == ')' {
                            if paren_count > 0 {
                                paren_count -= 1;
                                if paren_count == 0 {
                                    i += 1;
                                    break;
                                }
                            }
                        } else if paren_count == 0 && (chars[i] == '.' || chars[i] == '#' || chars[i] == '[' || chars[i] == ':') {
                            break;
                        }
                        i += 1;
                    }
                    let pseudo_str: String = chars[start..i].iter().collect();
                    if is_double || pseudo_str.eq_ignore_ascii_case("before") || pseudo_str.eq_ignore_ascii_case("after") {
                        sel.pseudo_element = Some(pseudo_str.to_ascii_lowercase());
                    } else {
                        let pseudo = parse_pseudo_class(&pseudo_str).unwrap_or(PseudoClass::Unsupported);
                        sel.pseudos.push(pseudo);
                    }
                }
                _ => {
                    i += 1;
                }
            }
        }

        Some(sel)
    }

    pub fn matches(&self, el: &Element, chain: &[&Element]) -> bool {
        self.matches_with_siblings(el, chain, &[])
    }

    pub fn matches_with_siblings(&self, el: &Element, chain: &[&Element], prev_siblings: &[&Element]) -> bool {
        if self.pseudo_element.is_some() {
            return false;
        }

        // Match tag
        if let Some(ref tag) = self.tag {
            if !el.tag.eq_ignore_ascii_case(tag) {
                return false;
            }
        }

        // Match id
        if let Some(ref id) = self.id {
            if &el.id != id {
                return false;
            }
        }

        // Match classes
        for req_class in &self.classes {
            let has_class = el.class_name
                .split_ascii_whitespace()
                .any(|c| c.eq_ignore_ascii_case(req_class));
            if !has_class {
                return false;
            }
        }

        // Match attributes
        for attr in &self.attributes {
            if !attr.matches(el) {
                return false;
            }
        }

        // Match pseudo-classes
        for pseudo in &self.pseudos {
            match pseudo {
                PseudoClass::Is(selectors) | PseudoClass::Where(selectors) => {
                    let matched_any = selectors.iter().any(|s| s.matches_with_siblings(chain, prev_siblings));
                    if !matched_any {
                        return false;
                    }
                }
                PseudoClass::Not(selectors) => {
                    let matched_any = selectors.iter().any(|s| s.matches_with_siblings(chain, prev_siblings));
                    if matched_any {
                        return false;
                    }
                }
                PseudoClass::Root => {
                    // In HTML, :root corresponds to the <html> document root element
                    if !el.tag.eq_ignore_ascii_case("html") {
                        return false;
                    }
                }
                PseudoClass::Empty => {
                    let is_empty = el.children.iter().all(|child| match child {
                        crate::html5::node::Node::Text(t) => t.text.trim().is_empty(),
                        crate::html5::node::Node::Element(_) => false,
                    });
                    if !is_empty {
                        return false;
                    }
                }
                PseudoClass::Link | PseudoClass::Visited | PseudoClass::AnyLink => {
                    let has_href = !el.href.is_empty() || get_attr(&el.attrs_raw, "href").is_some();
                    if !el.tag.eq_ignore_ascii_case("a") || !has_href {
                        return false;
                    }
                }
                PseudoClass::FirstChild => {
                    if !prev_siblings.is_empty() {
                        return false;
                    }
                }
                PseudoClass::Hover | PseudoClass::Active | PseudoClass::Focus | PseudoClass::Unsupported => {
                    return false;
                }
            }
        }

        true
    }
}

impl AttributeSelector {
    pub fn matches(&self, el: &Element) -> bool {
        // Special-case class, id, href if already parsed or check attrs_raw
        let val_opt = match self.name.to_ascii_lowercase().as_str() {
            "class" if !el.class_name.is_empty() => Some(el.class_name.as_str()),
            "id" if !el.id.is_empty() => Some(el.id.as_str()),
            "href" if !el.href.is_empty() => Some(el.href.as_str()),
            _ => get_attr(&el.attrs_raw, &self.name),
        };

        match &self.matcher {
            AttrMatcher::Exists => val_opt.is_some(),
            AttrMatcher::Exact(expected) => {
                val_opt.map(|v| v == expected).unwrap_or(false)
            }
            AttrMatcher::ExactCaseInsensitive(expected) => {
                val_opt.map(|v| v.eq_ignore_ascii_case(expected)).unwrap_or(false)
            }
            AttrMatcher::Includes(expected) => {
                val_opt.map(|v| v.split_ascii_whitespace().any(|word| word == expected)).unwrap_or(false)
            }
            AttrMatcher::Prefix(expected) => {
                val_opt.map(|v| v.starts_with(expected)).unwrap_or(false)
            }
            AttrMatcher::Suffix(expected) => {
                val_opt.map(|v| v.ends_with(expected)).unwrap_or(false)
            }
            AttrMatcher::Substring(expected) => {
                val_opt.map(|v| v.contains(expected)).unwrap_or(false)
            }
        }
    }
}

fn parse_attribute_selector(content: &str) -> Option<AttributeSelector> {
    let content = content.trim();
    if content.is_empty() {
        return None;
    }

    // Check operators: ~=, ^=, $=, *=, =
    let operators = [
        ("~=", 2),
        ("^=", 2),
        ("$=", 2),
        ("*=", 2),
        ("=", 1),
    ];

    for (op, op_len) in operators {
        if let Some(pos) = content.find(op) {
            let name = content[..pos].trim().to_ascii_lowercase();
            let mut val_part = content[pos + op_len..].trim();

            let mut case_insensitive = false;
            // Check for case-insensitive flag 'i' or 'I' at the end: e.g. [attr="val" i]
            if val_part.ends_with('i') || val_part.ends_with('I') {
                let trimmed = val_part[..val_part.len() - 1].trim_end();
                if trimmed.ends_with('"') || trimmed.ends_with('\'') || !trimmed.is_empty() {
                    case_insensitive = true;
                    val_part = trimmed;
                }
            }

            // Strip quotes from val_part
            let clean_val = if (val_part.starts_with('"') && val_part.ends_with('"'))
                || (val_part.starts_with('\'') && val_part.ends_with('\''))
            {
                if val_part.len() >= 2 {
                    &val_part[1..val_part.len() - 1]
                } else {
                    ""
                }
            } else {
                val_part
            };

            let matcher = match op {
                "=" if case_insensitive => AttrMatcher::ExactCaseInsensitive(clean_val.to_string()),
                "=" => AttrMatcher::Exact(clean_val.to_string()),
                "~=" => AttrMatcher::Includes(clean_val.to_string()),
                "^=" => AttrMatcher::Prefix(clean_val.to_string()),
                "$=" => AttrMatcher::Suffix(clean_val.to_string()),
                "*=" => AttrMatcher::Substring(clean_val.to_string()),
                _ => return None,
            };

            return Some(AttributeSelector { name, matcher });
        }
    }

    // Bare attribute name [disabled]
    Some(AttributeSelector {
        name: content.to_ascii_lowercase(),
        matcher: AttrMatcher::Exists,
    })
}

fn parse_pseudo_class(content: &str) -> Option<PseudoClass> {
    let content = content.trim();
    if content.eq_ignore_ascii_case("empty") {
        return Some(PseudoClass::Empty);
    }
    if content.eq_ignore_ascii_case("root") {
        return Some(PseudoClass::Root);
    }
    if content.eq_ignore_ascii_case("link") {
        return Some(PseudoClass::Link);
    }
    if content.eq_ignore_ascii_case("visited") {
        return Some(PseudoClass::Visited);
    }
    if content.eq_ignore_ascii_case("any-link") {
        return Some(PseudoClass::AnyLink);
    }
    if content.eq_ignore_ascii_case("first-child") {
        return Some(PseudoClass::FirstChild);
    }
    if content.eq_ignore_ascii_case("hover") {
        return Some(PseudoClass::Hover);
    }
    if content.eq_ignore_ascii_case("active") {
        return Some(PseudoClass::Active);
    }
    if content.eq_ignore_ascii_case("focus") || content.eq_ignore_ascii_case("focus-visible") {
        return Some(PseudoClass::Focus);
    }

    if let Some(paren_pos) = content.find('(') {
        if !content.ends_with(')') {
            return None;
        }
        let pseudo_name = content[..paren_pos].trim().to_ascii_lowercase();
        let inner = &content[paren_pos + 1..content.len() - 1];

        // Split inner selector list by top-level commas
        let sub_sel_strings = split_selector_list(inner);
        let mut selectors = Vec::new();
        for sub_str in sub_sel_strings {
            if let Some(c) = ComplexSelector::parse(&sub_str) {
                selectors.push(c);
            }
        }

        match pseudo_name.as_str() {
            "is" => Some(PseudoClass::Is(selectors)),
            "where" => Some(PseudoClass::Where(selectors)),
            "not" => Some(PseudoClass::Not(selectors)),
            _ => None,
        }
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::html5::node::Element;

    fn make_el(tag: &str, id: &str, class: &str, attrs_raw: &str) -> Element {
        Element {
            tag: tag.to_string(),
            id: id.to_string(),
            class_name: class.to_string(),
            href: String::new(),
            attrs_raw: attrs_raw.to_string(),
            children: Vec::new(),
            event_listeners: Vec::new(),
        }
    }

    #[test]
    fn test_split_selector_list() {
        let list = split_selector_list("h1, :is(h2, h3), div > p");
        assert_eq!(list, vec!["h1", ":is(h2, h3)", "div > p"]);
    }

    #[test]
    fn test_is_pseudo() {
        let h2 = make_el("h2", "", "", "");
        let p = make_el("p", "", "", "");
        let sel = ComplexSelector::parse(":is(h1, h2, h3)").unwrap();
        assert!(sel.matches(&[&h2]));
        assert!(!sel.matches(&[&p]));
    }

    #[test]
    fn test_where_pseudo() {
        let btn = make_el("button", "", "btn primary", "");
        let sel = ComplexSelector::parse(":where(button.primary, a.button)").unwrap();
        assert!(sel.matches(&[&btn]));
    }

    #[test]
    fn test_not_pseudo_list() {
        let div = make_el("div", "", "normal", "");
        let skip = make_el("div", "", "skip", "");
        let sel = ComplexSelector::parse("div:not(.skip, .ignore)").unwrap();
        assert!(sel.matches(&[&div]));
        assert!(!sel.matches(&[&skip]));
    }

    #[test]
    fn test_attr_case_insensitive() {
        let input = make_el("input", "", "", "type=\"SUBMIT\"");
        let sel = ComplexSelector::parse("input[type=\"submit\" i]").unwrap();
        assert!(sel.matches(&[&input]));

        let sel_sensitive = ComplexSelector::parse("input[type=\"submit\"]").unwrap();
        assert!(!sel_sensitive.matches(&[&input]));
    }

    #[test]
    fn test_child_and_descendant() {
        let section = make_el("section", "main", "", "");
        let div = make_el("div", "", "card", "");
        let p = make_el("p", "", "", "");

        let sel_child = ComplexSelector::parse("section#main > div.card > p").unwrap();
        assert!(sel_child.matches(&[&section, &div, &p]));

        let sel_descendant = ComplexSelector::parse("section#main p").unwrap();
        assert!(sel_descendant.matches(&[&section, &div, &p]));

        let other = make_el("span", "", "", "");
        assert!(!sel_child.matches(&[&section, &div, &other]));
    }

    #[test]
    fn test_root_pseudo() {
        let html = make_el("html", "", "", "");
        let body = make_el("body", "", "", "");
        let sel = ComplexSelector::parse(":root").unwrap();
        assert!(sel.matches(&[&html]));
        assert!(!sel.matches(&[&html, &body]));
    }

    #[test]
    fn test_sibling_combinators() {
        let p1 = make_el("p", "", "", "");
        let p2 = make_el("p", "", "", "");
        let div = make_el("div", "", "", "");
        let span = make_el("span", "", "", "");

        let sel_next = ComplexSelector::parse("p + p").unwrap();
        assert!(sel_next.matches_with_siblings(&[&p2], &[&p1]));
        assert!(!sel_next.matches_with_siblings(&[&p1], &[]));
        assert!(!sel_next.matches_with_siblings(&[&div], &[&p1]));

        let sel_subsequent = ComplexSelector::parse("p ~ span").unwrap();
        assert!(sel_subsequent.matches_with_siblings(&[&span], &[&p1, &div]));
        assert!(!sel_subsequent.matches_with_siblings(&[&span], &[&div]));
    }
}


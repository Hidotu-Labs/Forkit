pub mod color;
pub mod selector;

#[derive(Debug, Clone)]
pub struct CssRule {
    pub selector: String, // e.g. "h1", ":is(h1, h2)", "div > p"
    pub properties: Vec<(String, String)>,
    pub parsed_selector: Option<selector::ComplexSelector>,
    pub is_before: bool,
    pub before_base: Option<selector::ComplexSelector>,
}

impl CssRule {
    pub fn new(selector: String, properties: Vec<(String, String)>) -> Self {
        let s_trimmed = selector.trim();
        let is_before = s_trimmed.ends_with("::before") || s_trimmed.ends_with(":before");
        let (parsed_selector, before_base) = if is_before {
            let base_sel = s_trimmed.trim_end_matches("::before").trim_end_matches(":before").trim();
            (None, selector::ComplexSelector::parse(base_sel))
        } else {
            (selector::ComplexSelector::parse(s_trimmed), None)
        };
        CssRule {
            selector,
            properties,
            parsed_selector,
            is_before,
            before_base,
        }
    }

    #[inline]
    pub fn matches_element(&self, chain: &[&crate::html5::node::Element], siblings: &[&crate::html5::node::Element]) -> bool {
        if let Some(ref complex) = self.parsed_selector {
            complex.matches_with_siblings(chain, siblings)
        } else {
            false
        }
    }

    #[inline]
    pub fn matches_before(&self, chain: &[&crate::html5::node::Element], siblings: &[&crate::html5::node::Element]) -> bool {
        if let Some(ref base) = self.before_base {
            base.matches_with_siblings(chain, siblings)
        } else {
            false
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Stylesheet {
    pub rules: Vec<CssRule>,
}

/// Simple helper to extract a property value from an inline style string.
/// e.g. "color: red; background: blue" -> "red" for "color"
pub fn get_style_prop<'a>(style: &'a str, prop: &str) -> Option<&'a str> {
    for part in style.split(';') {
        let mut kv = part.split(':');
        let k = kv.next()?.trim();
        let v = kv.next()?.trim();
        if k.is_empty() || v.is_empty() { continue; }
        if k.eq_ignore_ascii_case(prop) {
            return Some(v);
        }
    }
    None
}

/// Parse declarations inside a CSS rule body "{ k: v; ... }"
fn parse_declarations(body: &str) -> Vec<(String, String)> {
    let mut properties = Vec::new();
    for decl in body.split(';') {
        let mut kv = decl.splitn(2, ':');
        let k = kv.next().unwrap_or("").trim();
        let v = kv.next().unwrap_or("").trim();
        if !k.is_empty() && !v.is_empty() {
            let key = if k.starts_with("--") {
                k.to_string()
            } else {
                k.to_lowercase()
            };
            properties.push((key, v.to_string()));
        }
    }
    properties
}

fn should_include_media(at_head: &str) -> bool {
    let lower = at_head.to_ascii_lowercase();
    if lower.contains("print") {
        return false;
    }
    if lower.contains("prefers-color-scheme: light") {
        return false;
    }
    if lower.contains("prefers-contrast") {
        return false;
    }
    let is_mobile_only = lower.contains("width <= 1000px")
        || lower.contains("width <= 800px")
        || lower.contains("width <= 600px")
        || lower.contains("max-width: 1000px")
        || lower.contains("max-width: 800px")
        || lower.contains("max-width: 600px");
    if is_mobile_only {
        return false;
    }
    true
}

fn parse_rules_from_str(css: &str, rules: &mut Vec<CssRule>) {
    let mut chars = css.char_indices().peekable();
    let len = css.len();

    while let Some(&(i, c)) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }

        if c == '@' {
            // Find whether ';' or '{' comes first
            let mut end_pos = None;
            let mut is_block = false;
            let mut iter = chars.clone();
            while let Some((pos, ch)) = iter.next() {
                if ch == ';' {
                    end_pos = Some(pos);
                    is_block = false;
                    break;
                } else if ch == '{' {
                    end_pos = Some(pos);
                    is_block = true;
                    break;
                }
            }

            if let Some(pos) = end_pos {
                if !is_block {
                    // Skip to ';'
                    while let Some((idx, _)) = chars.next() {
                        if idx == pos { break; }
                    }
                    continue;
                } else {
                    let at_head = css[i..pos].trim();
                    // Advance chars past the opening '{'
                    while let Some((idx, _)) = chars.next() {
                        if idx == pos { break; }
                    }
                    // Find matching '}'
                    let mut depth = 1usize;
                    let block_start = chars.peek().map(|&(p, _)| p).unwrap_or(len);
                    let mut block_end = block_start;
                    while let Some((idx, ch)) = chars.next() {
                        if ch == '{' {
                            depth += 1;
                        } else if ch == '}' {
                            depth -= 1;
                            if depth == 0 {
                                block_end = idx;
                                break;
                            }
                        }
                    }
                    let inner_block = if block_end >= block_start {
                        &css[block_start..block_end]
                    } else {
                        ""
                    };

                    if at_head.starts_with("@media") {
                        if should_include_media(at_head) {
                            parse_rules_from_str(inner_block, rules);
                        }
                    }
                    continue;
                }
            } else {
                break;
            }
        }

        // Regular rule: read selector up to '{'
        let sel_start = i;
        let mut brace_pos = None;
        while let Some((pos, ch)) = chars.next() {
            if ch == '{' {
                brace_pos = Some(pos);
                break;
            }
        }

        if let Some(pos) = brace_pos {
            let selector = css[sel_start..pos].trim();
            let mut depth = 1usize;
            let body_start = chars.peek().map(|&(p, _)| p).unwrap_or(len);
            let mut body_end = body_start;
            while let Some((idx, ch)) = chars.next() {
                if ch == '{' {
                    depth += 1;
                } else if ch == '}' {
                    depth -= 1;
                    if depth == 0 {
                        body_end = idx;
                        break;
                    }
                }
            }
            let body = if body_end >= body_start {
                &css[body_start..body_end]
            } else {
                ""
            };

            if !selector.is_empty() && !body.is_empty() {
                let properties = parse_declarations(body);
                for s in selector::split_selector_list(selector) {
                    let s_trimmed = s.trim();
                    if !s_trimmed.is_empty() {
                        rules.push(CssRule::new(s_trimmed.to_string(), properties.clone()));
                    }
                }
            }
        } else {
            break;
        }
    }
}

/// A lightweight CSS parser for <style> tags and stylesheets.
pub fn parse_stylesheet(css: &str) -> Stylesheet {
    let mut rules = Vec::new();
    
    // Strip /* ... */ CSS comments
    let mut cleaned_css = String::with_capacity(css.len());
    let mut chars = css.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '/' && chars.peek() == Some(&'*') {
            chars.next(); // consume '*'
            while let Some(nc) = chars.next() {
                if nc == '*' && chars.peek() == Some(&'/') {
                    chars.next(); // consume '/'
                    break;
                }
            }
        } else {
            cleaned_css.push(c);
        }
    }

    parse_rules_from_str(&cleaned_css, &mut rules);
    Stylesheet { rules }
}

/// Resolves `var(--name)` and `var(--name, fallback)` references within a CSS value string.
/// Supports nested `var(...)`, e.g. `var(--color, var(--fallback, blue))`.
/// `lookup` is called with the variable name to find its defined value.
pub fn resolve_var<F>(val: &str, lookup: &F) -> String
where
    F: Fn(&str) -> Option<String>,
{
    if !val.contains("var(") {
        return val.to_string();
    }

    let mut result = String::new();
    let chars: Vec<char> = val.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        if i + 4 <= len && &chars[i..i + 4] == &['v', 'a', 'r', '('] {
            // Find the matching ')' for this var(...)
            let start_inner = i + 4;
            let mut paren_depth = 1;
            let mut end_inner = start_inner;
            while end_inner < len {
                if chars[end_inner] == '(' {
                    paren_depth += 1;
                } else if chars[end_inner] == ')' {
                    paren_depth -= 1;
                    if paren_depth == 0 {
                        break;
                    }
                }
                end_inner += 1;
            }

            if paren_depth == 0 {
                let inner_str: String = chars[start_inner..end_inner].iter().collect();
                // Split variable name and fallback at top-level comma
                let mut var_name = String::new();
                let mut fallback: Option<String> = None;
                let mut inner_depth = 0;
                let mut found_comma = false;

                for c in inner_str.chars() {
                    if c == '(' {
                        inner_depth += 1;
                    } else if c == ')' && inner_depth > 0 {
                        inner_depth -= 1;
                    }

                    if c == ',' && inner_depth == 0 && !found_comma {
                        found_comma = true;
                        continue;
                    }

                    if found_comma {
                        fallback.get_or_insert_with(String::new).push(c);
                    } else {
                        var_name.push(c);
                    }
                }

                let var_name = var_name.trim();
                let resolved_fallback = fallback.map(|fb| resolve_var(fb.trim(), lookup));

                // Lookup variable
                let resolved_val = if let Some(v) = lookup(var_name) {
                    // Resolve nested vars in the variable value itself
                    resolve_var(v.trim(), lookup)
                } else if let Some(fb) = resolved_fallback {
                    fb
                } else {
                    String::new()
                };

                result.push_str(&resolved_val);
                i = end_inner + 1;
            } else {
                result.push(chars[i]);
                i += 1;
            }
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_resolve_var_simple() {
        let mut vars = HashMap::new();
        vars.insert("--primary".to_string(), "#3b82f6".to_string());
        vars.insert("--space".to_string(), "16px".to_string());

        let lookup = |name: &str| vars.get(name).cloned();

        assert_eq!(resolve_var("var(--primary)", &lookup), "#3b82f6");
        assert_eq!(resolve_var("10px var(--space) 20px", &lookup), "10px 16px 20px");
    }

    #[test]
    fn test_resolve_var_fallback() {
        let vars: HashMap<String, String> = HashMap::new();
        let lookup = |name: &str| vars.get(name).cloned();

        assert_eq!(resolve_var("var(--missing, #ef4444)", &lookup), "#ef4444");
        assert_eq!(resolve_var("var(--missing, var(--missing-2, green))", &lookup), "green");
    }

    #[test]
    fn test_resolve_var_nested() {
        let mut vars = HashMap::new();
        vars.insert("--accent".to_string(), "var(--primary)".to_string());
        vars.insert("--primary".to_string(), "#7c3aed".to_string());

        let lookup = |name: &str| vars.get(name).cloned();

        assert_eq!(resolve_var("var(--accent)", &lookup), "#7c3aed");
    }

    #[test]
    fn test_parse_stylesheet_with_at_rules() {
        let css = r#"
            @charset "UTF-8";
            :root {
                --main-bg: #fff;
                --main-pad: 20px;
            }
            @font-face {
                font-family: "CustomFont";
                src: url("font.woff2");
            }
            #header {
                background: var(--main-bg);
                padding: var(--main-pad);
            }
            @media (width <= 1000px) {
                #header {
                    padding: 10px;
                }
            }
            #footer {
                color: #333;
            }
        "#;
        let sheet = parse_stylesheet(css);
        assert!(!sheet.rules.is_empty());
        let root_rule = sheet.rules.iter().find(|r| r.selector == ":root");
        assert!(root_rule.is_some(), "Must parse :root rule");
        assert!(root_rule.unwrap().properties.iter().any(|(k, v)| k == "--main-bg" && v == "#fff"));

        let header_rule = sheet.rules.iter().find(|r| r.selector == "#header");
        assert!(header_rule.is_some(), "Must parse #header rule");

        let footer_rule = sheet.rules.iter().find(|r| r.selector == "#footer");
        assert!(footer_rule.is_some(), "Must parse #footer rule");
    }

    #[test]
    fn test_iana_css_rules() {
        let css = r#"
            :root {
                --color-background: oklch(0.95 0.005 220);
                --color-chrome: var(--color-background);
                --page-margin: 50px;
                --font-size-base: 16px;
            }
            #header {
                max-width: calc(1200px - var(--page-margin) * 2);
                margin: 0 auto;
                background: var(--color-chrome);
            }
            #header #logo {
                float: left;
            }
            #header .navigation {
                float: right;
            }
            .breadcrumb li a::before {
                content: "←";
            }
        "#;
        let sheet = parse_stylesheet(css);
        assert!(sheet.rules.iter().any(|r| r.selector == ":root"));
        assert!(sheet.rules.iter().any(|r| r.selector == "#header"));
        assert!(sheet.rules.iter().any(|r| r.selector == "#header #logo"));
        assert!(sheet.rules.iter().any(|r| r.selector == "#header .navigation"));
        assert!(sheet.rules.iter().any(|r| r.selector == ".breadcrumb li a::before"));
    }

    #[test]
    fn test_matched_iana_rules() {
        let html = std::fs::read_to_string("/home/offihito/.gemini/antigravity/brain/c4ae3338-513c-4cd3-a3d8-f293f8ddaca7/scratch/iana.html").unwrap();
        let css = std::fs::read_to_string("/home/offihito/.gemini/antigravity/brain/c4ae3338-513c-4cd3-a3d8-f293f8ddaca7/scratch/iana.css").unwrap();
        let dom = crate::html5::parser::parse_dom(&html);
        let sheet = parse_stylesheet(&css);

        fn visit<'a>(node: &'a crate::html5::node::Node, chain: &mut Vec<&'a crate::html5::node::Element>, sheet: &Stylesheet) {
            if let crate::html5::node::Node::Element(el) = node {
                chain.push(el);
                for rule in &sheet.rules {
                    if rule.matches_element(chain, &[]) {
                        for (p, v) in &rule.properties {
                            if p.contains("background") || p.contains("color") {
                                if el.tag == "body" || el.tag == "header" || el.tag == "article" {
                                    println!("<{}> [{}] matched '{}': {} = {}", el.tag, el.class_name, rule.selector, p, v);
                                }
                            }
                        }
                    }
                }
                for child in &el.children {
                    visit(child, chain, sheet);
                }
                chain.pop();
            }
        }

        visit(&dom, &mut Vec::new(), &sheet);
    }
}

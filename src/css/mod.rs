pub mod color;

#[derive(Debug, Clone)]
pub struct CssRule {
    pub selector: String, // For now, just tag names like "h1", "div"
    pub properties: Vec<(String, String)>,
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

/// A lightweight CSS parser for <style> tags.
/// Supports simple "selector { prop: val; }" blocks.
pub fn parse_stylesheet(css: &str) -> Stylesheet {
    let mut rules = Vec::new();
    
    // Split by blocks ending with '}'
    for block in css.split('}') {
        let mut parts = block.split('{');
        let selector = parts.next().unwrap_or("").trim();
        let body = parts.next().unwrap_or("").trim();
        
        if selector.is_empty() || body.is_empty() { continue; }
        
        let mut properties = Vec::new();
        for decl in body.split(';') {
            let mut kv = decl.split(':');
            let k = kv.next().unwrap_or("").trim();
            let v = kv.next().unwrap_or("").trim();
            if !k.is_empty() && !v.is_empty() {
                properties.push((k.to_lowercase(), v.to_string()));
            }
        }
        
        // Multi-selector support (e.g. "h1, h2 {}")
        for s in selector.split(',') {
            let s_trimmed = s.trim();
            if !s_trimmed.is_empty() {
                rules.push(CssRule {
                    selector: s_trimmed.to_lowercase(),
                    properties: properties.clone(),
                });
            }
        }
    }
    
    Stylesheet { rules }
}

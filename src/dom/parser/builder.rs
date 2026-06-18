use crate::dom::node::{Node, Element, TextNode, Style};
use crate::dom::css::{apply_tag_defaults, apply_inline, StyleSheet, apply_cascade};
use crate::net;

use super::attr::get_attr;
use super::lexer::{Lexer, TokKind};
use super::tags::{is_void, is_skip, is_style_harvest, is_raw_text};

pub fn parse_with_sheets(html: &str, base_url: &str) -> (Node, Vec<StyleSheet>) {
    let mut lexer = Lexer::new(html);

    let mut stack: Vec<Element> = vec![Element {
        tag:        "#document".into(),
        id:         String::new(),
        class_name: String::new(),
        style_attr: String::new(),
        attrs_raw:  String::new(),
        style:      Style { display_block: true, ..Default::default() },
        children:   Vec::new(),
        event_listeners: Vec::new(),
    }];

    let mut style_texts: Vec<String> = Vec::new();

    loop {
        let tok = lexer.next_token();
        match tok.kind {
            TokKind::Eof => break,

            TokKind::Text => {
                if stack.len() <= 1 { continue; }
                let in_pre = stack.iter().any(|e| e.style.white_space_pre);
                if !in_pre && tok.attrs.chars().all(|c| c.is_ascii_whitespace()) {
                    // Collapse inter-element whitespace to a single space so that
                    // inline siblings (e.g. two <img> tags) get the standard HTML
                    // inter-element space.  The renderer will skip it when the cursor
                    // is already at the start of a line.
                    let parent_style = stack.last().unwrap().style.clone();
                    stack.last_mut().unwrap().children.push(Node::Text(TextNode {
                        text:  " ".to_string(),
                        style: parent_style,
                    }));
                    continue;
                }
                let parent_style = stack.last().unwrap().style.clone();
                stack.last_mut().unwrap().children.push(Node::Text(TextNode {
                    text:  tok.attrs,
                    style: parent_style,
                }));
            }

            TokKind::Open | TokKind::SelfClose => {
                let tag = tok.tag.as_str();

                if tag == "style" && tok.kind != TokKind::SelfClose {
                    style_texts.push(lexer.read_raw_until("style"));
                    continue;
                }

                if is_skip(tag) {
                    if !is_void(tag) && tok.kind != TokKind::SelfClose {
                        let mut depth = 1usize;
                        while depth > 0 {
                            let t2 = lexer.next_token();
                            match t2.kind {
                                TokKind::Eof                            => break,
                                TokKind::Open  if t2.tag == tok.tag    => depth += 1,
                                TokKind::Close if t2.tag == tok.tag    => depth -= 1,
                                _                                       => {}
                            }
                        }
                    }
                    continue;
                }

                if is_style_harvest(tag) && !is_void(tag) && tok.kind != TokKind::SelfClose {
                    let mut depth = 1usize;
                    while depth > 0 {
                        let t2 = lexer.next_token();
                        match t2.kind {
                            TokKind::Eof                            => break,
                            TokKind::Open  if t2.tag == tok.tag    => depth += 1,
                            TokKind::Close if t2.tag == tok.tag    => depth -= 1,
                            TokKind::Open if t2.tag == "style" => {
                                style_texts.push(lexer.read_raw_until("style"));
                            }
                            TokKind::Open | TokKind::SelfClose if t2.tag == "link" => {
                                let rel  = get_attr(&t2.attrs, "rel").unwrap_or("").to_ascii_lowercase();
                                let href = get_attr(&t2.attrs, "href").unwrap_or("");
                                if rel.contains("stylesheet") && !href.is_empty() {
                                    let css_url = net::resolve_url(href, base_url);
                                    match net::fetch_url(&css_url) {
                                        Ok((final_css_url, css)) => {
                                            // Rewrite relative url() references in the CSS
                                            // so they resolve against the CSS file's location,
                                            // not the page URL.
                                            let css = rewrite_css_urls(&css, &final_css_url);
                                            style_texts.push(css);
                                        }
                                        Err(e) => eprintln!("CSS fetch {css_url}: {e}"),
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    continue;
                }

                let parent_style = stack.last().unwrap().style.clone();
                let mut el = Element {
                    tag:        tok.tag.clone(),
                    id:         get_attr(&tok.attrs, "id").unwrap_or("").to_owned(),
                    class_name: get_attr(&tok.attrs, "class").unwrap_or("").to_owned(),
                    style_attr: get_attr(&tok.attrs, "style").unwrap_or("").to_owned(),
                    attrs_raw:  tok.attrs.clone(),
                    style:      parent_style,
                    children:   Vec::new(),
                    event_listeners: Vec::new(),
                };
                apply_tag_defaults(&mut el);
                let inline = el.style_attr.clone();
                apply_inline(&inline, &mut el.style);

                // Inline SVG: capture the raw markup as a single Text child so
                // the renderer can hand it directly to the SVG image pipeline.
                // We do NOT descend into SVG children — this prevents SVG-internal
                // <style> blocks and <title> elements from polluting the page.
                if tag == "svg" && tok.kind != TokKind::SelfClose {
                    let inner = lexer.read_raw_until("svg");
                    // Re-wrap with the opening <svg ...> tag so nanosvg sees a valid document.
                    let svg_markup = format!("<svg {}>{}</svg>", tok.attrs, inner);
                    el.children.push(Node::Text(TextNode {
                        text:  svg_markup,
                        style: el.style.clone(),
                    }));
                    stack.last_mut().unwrap().children.push(Node::Element(el));
                    continue;
                }

                if is_raw_text(tag) && tok.kind != TokKind::SelfClose {
                    let raw = lexer.read_raw_until(tag);
                    el.children.push(Node::Text(TextNode {
                        text:  raw,
                        style: el.style.clone(),
                    }));
                    stack.last_mut().unwrap().children.push(Node::Element(el));
                    continue;
                }

                if tok.kind == TokKind::SelfClose || is_void(tag) {
                    stack.last_mut().unwrap().children.push(Node::Element(el));
                } else {
                    match tag {
                        "dt" | "dd" => {
                            loop {
                                let top = stack.last().map(|e| e.tag.as_str()).unwrap_or("");
                                if top == "dt" || top == "dd" {
                                    let fin = stack.pop().unwrap();
                                    stack.last_mut().unwrap().children.push(Node::Element(fin));
                                } else {
                                    break;
                                }
                            }
                        }
                        "li" => {
                            if stack.last().map(|e| e.tag.as_str()) == Some("li") {
                                let fin = stack.pop().unwrap();
                                stack.last_mut().unwrap().children.push(Node::Element(fin));
                            }
                        }
                        _ if el.style.display_block => {
                            if stack.last().map(|e| e.tag.as_str()) == Some("p") {
                                let fin = stack.pop().unwrap();
                                stack.last_mut().unwrap().children.push(Node::Element(fin));
                            }
                        }
                        _ => {}
                    }
                    stack.push(el);
                }
            }

            TokKind::Close => {
                if let Some(pos) = stack.iter().rposition(|e| e.tag == tok.tag) {
                    while stack.len() > pos + 1 {
                        let fin = stack.pop().unwrap();
                        stack.last_mut().unwrap().children.push(Node::Element(fin));
                    }
                    if stack.len() > 1 {
                        let fin = stack.pop().unwrap();
                        stack.last_mut().unwrap().children.push(Node::Element(fin));
                    }
                }
            }
        }
    }

    while stack.len() > 1 {
        let fin = stack.pop().unwrap();
        stack.last_mut().unwrap().children.push(Node::Element(fin));
    }

    let mut root = Node::Element(stack.pop().unwrap());

    let sheets: Vec<StyleSheet> = if !style_texts.is_empty() {
        let parsed: Vec<StyleSheet> = style_texts.iter()
            .map(|css| StyleSheet::parse(css))
            .collect();
        apply_cascade(&mut root, &parsed);
        parsed
    } else {
        Vec::new()
    };

    (root, sheets)
}

/// Parse a simple HTML fragment string into a list of `Node`s.
///
/// Used by JS `innerHTML` setter.  Runs the same tokeniser / builder as the
/// full page parser but wraps the input in a temporary `<div>` to give
/// the builder a container, then extracts its children.
/// No external stylesheets are fetched; the cascade is not applied.
pub fn parse_fragment(html: &str) -> Vec<Node> {
    let wrapped = format!("<div>{}</div>", html);
    let (root, _) = parse_with_sheets(&wrapped, "about:blank");
    // root = #document > body? > div > children
    // Walk down to find the first <div> and return its children.
    fn find_div(node: &Node) -> Option<Vec<Node>> {
        if let Node::Element(el) = node {
            if el.tag == "div" {
                // Re-clone children. Can't move out because the tree is owned.
                return Some(el.children.iter().map(|c| clone_node(c)).collect());
            }
            for child in &el.children {
                if let Some(result) = find_div(child) {
                    return Some(result);
                }
            }
        }
        None
    }
    find_div(&root).unwrap_or_default()
}

fn clone_node(node: &Node) -> Node {
    match node {
        Node::Text(t) => Node::Text(crate::dom::node::TextNode {
            text:  t.text.clone(),
            style: t.style.clone(),
        }),
        Node::Element(e) => Node::Element(crate::dom::node::Element {
            tag:        e.tag.clone(),
            id:         e.id.clone(),
            class_name: e.class_name.clone(),
            style_attr: e.style_attr.clone(),
            attrs_raw:  e.attrs_raw.clone(),
            style:      e.style.clone(),
            children:   e.children.iter().map(|c| clone_node(c)).collect(),
            event_listeners: e.event_listeners.clone(),
        }),
    }
}
/// absolute URLs resolved against `css_base_url` (the URL the CSS was fetched from).
///
/// Already-absolute URLs (`http://`, `https://`, `data:`, `//`) are left unchanged.
fn rewrite_css_urls(css: &str, css_base_url: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let lower   = css.to_ascii_lowercase();
    let bytes   = css.as_bytes();
    let len     = css.len();
    let mut pos = 0;

    while pos < len {
        // Find next "url("
        match lower[pos..].find("url(") {
            None => {
                out.push_str(&css[pos..]);
                break;
            }
            Some(rel) => {
                let abs = pos + rel;
                // Copy everything up to (and including) "url("
                out.push_str(&css[pos..abs + 4]);
                let after_open = abs + 4;

                // Find the matching closing paren, tracking nesting and quotes.
                let mut depth: usize = 1;
                let mut in_quote: Option<u8> = None;
                let mut i = after_open;
                while i < len {
                    let b = bytes[i];
                    match in_quote {
                        Some(q) if b == q => { in_quote = None; }
                        Some(b'\\')       => { i += 1; } // skip escaped char
                        Some(_)           => {}
                        None => match b {
                            b'"' | b'\'' => { in_quote = Some(b); }
                            b'('         => { depth += 1; }
                            b')' => {
                                depth -= 1;
                                if depth == 0 { break; }
                            }
                            _ => {}
                        }
                    }
                    i += 1;
                }
                // `i` now points at the closing ')' (or end-of-string if malformed)
                let inner = &css[after_open..i];

                // Strip optional quotes from the inner URL
                let stripped = inner.trim();
                let url_str = if (stripped.starts_with('"') && stripped.ends_with('"'))
                    || (stripped.starts_with('\'') && stripped.ends_with('\''))
                {
                    &stripped[1..stripped.len() - 1]
                } else {
                    stripped
                };

                // Only rewrite if it's a relative URL
                let is_absolute = url_str.starts_with("http://")
                    || url_str.starts_with("https://")
                    || url_str.starts_with("data:")
                    || url_str.starts_with("//")
                    || url_str.starts_with('#')
                    || url_str.is_empty();

                if is_absolute {
                    // Leave as-is
                    out.push_str(inner);
                } else {
                    let resolved = net::resolve_url(url_str, css_base_url);
                    out.push_str(&resolved);
                }

                // Copy the closing ')' if present
                if i < len && bytes[i] == b')' {
                    out.push(')');
                    pos = i + 1;
                } else {
                    pos = i;
                }
            }
        }
    }

    out
}

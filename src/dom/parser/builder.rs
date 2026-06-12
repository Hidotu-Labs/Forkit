use crate::dom::node::{Node, Element, TextNode, Style};
use crate::dom::css::{apply_tag_defaults, apply_inline, StyleSheet, apply_cascade};
use crate::net;
use super::lexer::{Lexer, TokKind};

// ---------------------------------------------------------------------------
// Tag classification tables
// ---------------------------------------------------------------------------

const VOID_TAGS: &[&str] = &[
    "area","base","br","col","embed","hr","img","input",
    "link","meta","param","source","track","wbr",
];

/// Tags whose entire subtree is skipped (non-visual / scripting).
const SKIP_TAGS: &[&str] = &[
    "script","noscript","template","svg","math","title",
];

/// Tags whose subtree is skipped but whose `<style>` children are still harvested.
const STYLE_HARVEST_TAGS: &[&str] = &["head"];

/// Tags whose text content is read verbatim (no child tag parsing).
const RAW_TEXT_TAGS: &[&str] = &["pre", "textarea"];

fn is_void(tag: &str)           -> bool { VOID_TAGS.contains(&tag) }
fn is_skip(tag: &str)           -> bool { SKIP_TAGS.contains(&tag) }
fn is_style_harvest(tag: &str)  -> bool { STYLE_HARVEST_TAGS.contains(&tag) }
fn is_raw_text(tag: &str)       -> bool { RAW_TEXT_TAGS.contains(&tag) }

// ---------------------------------------------------------------------------
// Attribute extractor (pub so other modules can reuse it)
// ---------------------------------------------------------------------------

/// Extract the value of a named attribute from a raw attribute string.
pub fn get_attr<'a>(attrs: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("{}=", key);
    let lower  = attrs.to_ascii_lowercase();
    let start  = lower.find(needle.as_str())? + needle.len();
    let rest   = &attrs[start..];
    if rest.is_empty() { return None; }
    let quote = if rest.starts_with('"') || rest.starts_with('\'') {
        Some(rest.as_bytes()[0] as char)
    } else { None };
    let val_start = if quote.is_some() { &rest[1..] } else { rest };
    let end = if let Some(q) = quote {
        val_start.find(q).unwrap_or(val_start.len())
    } else {
        val_start.find(|c: char| c.is_ascii_whitespace() || c == '>')
                 .unwrap_or(val_start.len())
    };
    Some(&val_start[..end])
}

// ---------------------------------------------------------------------------
// DOM builder
// ---------------------------------------------------------------------------

/// Parse an HTML string into a `Node` tree rooted at `#document`.
/// `base_url` is used to resolve relative `<link rel="stylesheet">` hrefs.
pub fn parse(html: &str, base_url: &str) -> Node {
    let (node, _) = parse_with_sheets(html, base_url);
    node
}

/// Like [`parse`] but also returns the parsed `StyleSheet` list so the caller
/// can store them for re-applying the cascade each frame (e.g. for `:hover`).
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
    }];

    // Accumulate text content from all <style> elements for the cascade pass.
    let mut style_texts: Vec<String> = Vec::new();

    loop {
        let tok = lexer.next_token();
        match tok.kind {
            TokKind::Eof => break,

            TokKind::Text => {
                // Drop text that appears before any real element is opened
                // (e.g. bare-text preambles some servers send before <html>).
                if stack.len() <= 1 {
                    continue;
                }
                let in_pre = stack.iter().any(|e| e.style.white_space_pre);
                if !in_pre && tok.attrs.chars().all(|c| c.is_ascii_whitespace()) {
                    continue; // drop whitespace-only nodes outside <pre>
                }
                let parent_style = stack.last().unwrap().style.clone();
                stack.last_mut().unwrap().children.push(Node::Text(TextNode {
                    text:  tok.attrs,
                    style: parent_style,
                }));
            }

            TokKind::Open | TokKind::SelfClose => {
                let tag = tok.tag.as_str();

                // Capture <style> element text content for the cascade pass.
                if tag == "style" && tok.kind != TokKind::SelfClose {
                    let css_text = lexer.read_raw_until("style");
                    style_texts.push(css_text);
                    continue;
                }

                // Skip non-visual subtrees entirely
                if is_skip(tag) {
                    if !is_void(tag) && tok.kind != TokKind::SelfClose {
                        let mut depth = 1usize;
                        while depth > 0 {
                            let t2 = lexer.next_token();
                            match t2.kind {
                                TokKind::Eof   => break,
                                TokKind::Open  if t2.tag == tok.tag => depth += 1,
                                TokKind::Close if t2.tag == tok.tag => depth -= 1,
                                _ => {}
                            }
                        }
                    }
                    continue;
                }

                // For tags like <head>: skip visual rendering but harvest <style> blocks.
                if is_style_harvest(tag) && !is_void(tag) && tok.kind != TokKind::SelfClose {
                    let mut depth = 1usize;
                    while depth > 0 {
                        let t2 = lexer.next_token();
                        match t2.kind {
                            TokKind::Eof => break,
                            TokKind::Open if t2.tag == tok.tag => depth += 1,
                            TokKind::Close if t2.tag == tok.tag => depth -= 1,
                            TokKind::Open if t2.tag == "style" => {
                                // Harvest inline <style> CSS text.
                                let css_text = lexer.read_raw_until("style");
                                style_texts.push(css_text);
                                // read_raw_until already consumed </style>
                            }
                            TokKind::Open | TokKind::SelfClose if t2.tag == "link" => {
                                // Fetch external stylesheet: <link rel="stylesheet" href="...">
                                let rel  = get_attr(&t2.attrs, "rel").unwrap_or("").to_ascii_lowercase();
                                let href = get_attr(&t2.attrs, "href").unwrap_or("");
                                if rel == "stylesheet" && !href.is_empty() {
                                    let url = net::resolve_url(href, base_url);
                                    match net::fetch_url(&url) {
                                        Ok((_, css)) => style_texts.push(css),
                                        Err(e) => eprintln!("CSS fetch {url}: {e}"),
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
                };
                apply_tag_defaults(&mut el);
                let inline = el.style_attr.clone();
                apply_inline(&inline, &mut el.style);

                // Raw-text elements: slurp content as a single verbatim text node
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
                    // ── Implicit close rules (HTML5 optional-close tags) ────
                    // Some elements auto-close when a sibling of the same
                    // family is opened, e.g. <dt>/<dd> in a <dl>, <li> in a
                    // <ul>/<ol>, <p> before a block element.
                    match tag {
                        // <dt> and <dd> implicitly close any open <dt>/<dd>
                        // that hasn't been explicitly closed yet.
                        "dt" | "dd" => {
                            // Walk back and pop any open dt/dd (stop at dl or
                            // the document root so we don't break other structure).
                            while stack.len() > 1 {
                                let top = stack.last().map(|e| e.tag.as_str()).unwrap_or("");
                                if top == "dt" || top == "dd" {
                                    let fin = stack.pop().unwrap();
                                    stack.last_mut().unwrap().children.push(Node::Element(fin));
                                } else {
                                    break;
                                }
                            }
                        }
                        // <li> implicitly closes a previous open <li>
                        "li" => {
                            if stack.last().map(|e| e.tag.as_str()) == Some("li") {
                                let fin = stack.pop().unwrap();
                                stack.last_mut().unwrap().children.push(Node::Element(fin));
                            }
                        }
                        // <p> is implicitly closed by any block-level element
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

    // Drain remaining unclosed elements into the document root
    while stack.len() > 1 {
        let fin = stack.pop().unwrap();
        stack.last_mut().unwrap().children.push(Node::Element(fin));
    }

    let mut root = Node::Element(stack.pop().unwrap());

    // ── Cascade pass ────────────────────────────────────────────────────────
    // Parse all collected <style> text blocks and apply the cascade to the
    // fully-built DOM tree.  This runs after the tree is complete so that
    // selector matching can walk ancestors correctly.
    let sheets: Vec<StyleSheet> = if !style_texts.is_empty() {
        let parsed: Vec<StyleSheet> = style_texts
            .iter()
            .map(|css| StyleSheet::parse(css))
            .collect();
        apply_cascade(&mut root, &parsed);
        parsed
    } else {
        Vec::new()
    };

    (root, sheets)
}

// ---------------------------------------------------------------------------
// Page metadata extraction
// ---------------------------------------------------------------------------

/// Extract `(page_title, favicon_url)` from raw HTML without a full parse.
///
/// Scans only the `<head>` portion for speed.  Falls back to empty string /
/// `None` if the tags are absent.
pub fn extract_page_meta(html: &str, base_url: &str) -> (String, Option<String>) {
    // Work on a lowercase copy for case-insensitive tag/attribute matching,
    // but keep the original for extracting text content.
    let lower = html.to_ascii_lowercase();

    // ── page title ────────────────────────────────────────────────────────
    let title = find_title(html, &lower);

    // ── favicon ───────────────────────────────────────────────────────────
    let favicon = find_favicon(&lower, html, base_url);

    (title, favicon)
}

fn find_title(html: &str, lower: &str) -> String {
    let open = match lower.find("<title") {
        Some(p) => p,
        None    => return String::new(),
    };
    // skip to end of opening tag
    let after_open = match lower[open..].find('>') {
        Some(p) => open + p + 1,
        None    => return String::new(),
    };
    let close = match lower[after_open..].find("</title") {
        Some(p) => after_open + p,
        None    => return String::new(),
    };
    let raw = &html[after_open..close];
    // Decode HTML entities and collapse whitespace
    let decoded = crate::dom::parser::decode_entities(raw);
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn find_favicon(lower: &str, html: &str, base_url: &str) -> Option<String> {
    // We look for <link … rel="…icon…" … href="…"> tags.
    // Iterate over all <link occurrences and pick the best one.
    let mut best: Option<String> = None;

    let mut pos = 0;
    while let Some(rel_start) = lower[pos..].find("<link") {
        let abs = pos + rel_start;
        let tag_end = lower[abs..].find('>').map(|p| abs + p + 1).unwrap_or(lower.len());
        let tag_lower = &lower[abs..tag_end];
        let tag_orig  = &html[abs..tag_end.min(html.len())];

        let rel = get_attr(tag_lower, "rel").unwrap_or("");
        if rel.contains("icon") {
            if let Some(href) = get_attr(tag_orig, "href") {
                if !href.is_empty() {
                    let resolved = crate::net::resolve_url(href, base_url);
                    // Prefer explicit "icon" over "apple-touch-icon" etc.
                    if best.is_none() || rel == "icon" || rel == "shortcut icon" {
                        best = Some(resolved);
                    }
                }
            }
        }

        pos = tag_end;
        if pos >= lower.len() { break; }
    }

    // Fallback: try /favicon.ico on the same origin
    if best.is_none() {
        let origin = {
            if let Some(p) = base_url.find("://") {
                let rest = &base_url[p + 3..];
                let end  = rest.find('/').unwrap_or(rest.len());
                format!("{}://{}", &base_url[..p], &rest[..end])
            } else {
                String::new()
            }
        };
        if !origin.is_empty() {
            best = Some(format!("{}/favicon.ico", origin));
        }
    }

    best
}

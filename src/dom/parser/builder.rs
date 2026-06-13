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
                                    let url = net::resolve_url(href, base_url);
                                    match net::fetch_url(&url) {
                                        Ok((_, css)) => style_texts.push(css),
                                        Err(e)       => eprintln!("CSS fetch {url}: {e}"),
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

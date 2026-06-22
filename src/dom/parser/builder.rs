use crate::dom::node::{Node, Element, TextNode};

use super::attr::get_attr;
use super::lexer::{Lexer, TokKind};
use super::tags::{is_void, is_skip, is_raw_text};

pub fn parse_dom(html: &str) -> Node {
    let mut lexer = Lexer::new(html);

    let mut stack: Vec<Element> = vec![Element {
        tag:        "#document".into(),
        id:         String::new(),
        class_name: String::new(),
        href:       String::new(),
        attrs_raw:  String::new(),
        children:   Vec::new(),
        event_listeners: Vec::new(),
    }];

    loop {
        let tok = lexer.next_token();
        match tok.kind {
            TokKind::Eof => break,

            TokKind::Text => {
                if stack.len() <= 1 { continue; }
                if tok.attrs.chars().all(|c| c.is_ascii_whitespace()) {
                    stack.last_mut().unwrap().children.push(Node::Text(TextNode {
                        text:  " ".to_string(),
                    }));
                    continue;
                }
                stack.last_mut().unwrap().children.push(Node::Text(TextNode {
                    text:  tok.attrs,
                }));
            }

            TokKind::Open | TokKind::SelfClose => {
                let tag = tok.tag.as_str();

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

                let mut el = Element {
                    tag:        tok.tag.clone(),
                    id:         get_attr(&tok.attrs, "id").unwrap_or("").to_owned(),
                    class_name: get_attr(&tok.attrs, "class").unwrap_or("").to_owned(),
                    href:       get_attr(&tok.attrs, "href").unwrap_or("").to_owned(),
                    attrs_raw:  tok.attrs.clone(),
                    children:   Vec::new(),
                    event_listeners: Vec::new(),
                };

                if tag == "svg" && tok.kind != TokKind::SelfClose {
                    let inner = lexer.read_raw_until("svg");
                    let svg_markup = format!("<svg {}>{}</svg>", tok.attrs, inner);
                    el.children.push(Node::Text(TextNode {
                        text:  svg_markup,
                    }));
                    stack.last_mut().unwrap().children.push(Node::Element(el));
                    continue;
                }

                if is_raw_text(tag) && tok.kind != TokKind::SelfClose {
                    let raw = lexer.read_raw_until(tag);
                    el.children.push(Node::Text(TextNode {
                        text:  raw,
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

    Node::Element(stack.pop().unwrap())
}

pub fn parse_fragment(html: &str) -> Vec<Node> {
    let wrapped = format!("<div>{}</div>", html);
    let root = parse_dom(&wrapped);
    fn find_div(node: &Node) -> Option<Vec<Node>> {
        if let Node::Element(el) = node {
            if el.tag == "div" {
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
        }),
        Node::Element(e) => Node::Element(crate::dom::node::Element {
            tag:        e.tag.clone(),
            id:         e.id.clone(),
            class_name: e.class_name.clone(),
            href:       e.href.clone(),
            attrs_raw:  e.attrs_raw.clone(),
            children:   e.children.iter().map(|c| clone_node(c)).collect(),
            event_listeners: e.event_listeners.clone(),
        }),
    }
}

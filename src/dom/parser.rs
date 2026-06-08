use super::node::{Node, Element, TextNode, Style};
use super::css::{apply_tag_defaults, apply_inline};

// ---------------------------------------------------------------------------
// Tokenizer
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq)]
enum TokKind { Eof, Text, Open, Close, SelfClose }

struct Token {
    kind:  TokKind,
    tag:   String,
    /// Raw attribute string for element tokens, decoded text for text tokens.
    attrs: String,
}

struct Lexer<'a> {
    src: &'a [u8],
    pos: usize,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Self {
        Lexer { src: src.as_bytes(), pos: 0 }
    }

    fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }

    fn skip_ws(&mut self) {
        while self.pos < self.src.len() && self.src[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    fn read_while<F: Fn(u8) -> bool>(&mut self, f: F) -> String {
        let start = self.pos;
        while self.pos < self.src.len() && f(self.src[self.pos]) {
            self.pos += 1;
        }
        String::from_utf8_lossy(&self.src[start..self.pos]).into_owned()
    }

    fn next_token(&mut self) -> Token {
        if self.pos >= self.src.len() {
            return Token { kind: TokKind::Eof, tag: String::new(), attrs: String::new() };
        }

        if self.src[self.pos] != b'<' {
            // Text node
            let start = self.pos;
            while self.pos < self.src.len() && self.src[self.pos] != b'<' {
                self.pos += 1;
            }
            let raw = String::from_utf8_lossy(&self.src[start..self.pos]).into_owned();
            return Token {
                kind:  TokKind::Text,
                tag:   String::new(),
                attrs: decode_entities(&raw),
            };
        }

        self.pos += 1; // consume '<'

        // HTML comment <!-- … -->
        if self.src.get(self.pos..self.pos+3) == Some(b"!--") {
            self.pos += 3;
            while self.pos + 2 < self.src.len() {
                if &self.src[self.pos..self.pos+3] == b"-->" {
                    self.pos += 3;
                    break;
                }
                self.pos += 1;
            }
            return self.next_token();
        }

        // DOCTYPE / other <!…>
        if self.peek() == Some(b'!') {
            while self.pos < self.src.len() && self.src[self.pos] != b'>' {
                self.pos += 1;
            }
            if self.pos < self.src.len() { self.pos += 1; }
            return self.next_token();
        }

        // Closing tag </tag>
        if self.peek() == Some(b'/') {
            self.pos += 1;
            self.skip_ws();
            let tag = self.read_while(|c| c != b'>' && !c.is_ascii_whitespace())
                          .to_ascii_lowercase();
            while self.pos < self.src.len() && self.src[self.pos] != b'>' {
                self.pos += 1;
            }
            if self.pos < self.src.len() { self.pos += 1; }
            return Token { kind: TokKind::Close, tag, attrs: String::new() };
        }

        // Opening tag <tag attrs> or <tag attrs/>
        self.skip_ws();
        let tag = self.read_while(|c| {
            c != b'>' && c != b'/' && !c.is_ascii_whitespace()
        }).to_ascii_lowercase();

        // Read raw attribute string
        self.skip_ws();
        let mut attrs = String::new();
        let mut in_quote = false;
        let mut qchar = b'"';
        while self.pos < self.src.len() {
            let c = self.src[self.pos];
            if !in_quote && c == b'>' { break; }
            if !in_quote && c == b'/' && self.src.get(self.pos+1) == Some(&b'>') { break; }
            if !in_quote && (c == b'"' || c == b'\'') { in_quote = true; qchar = c; }
            else if in_quote && c == qchar             { in_quote = false; }
            attrs.push(c as char);
            self.pos += 1;
        }

        let kind = if self.peek() == Some(b'/') {
            self.pos += 1;
            TokKind::SelfClose
        } else {
            TokKind::Open
        };
        if self.peek() == Some(b'>') { self.pos += 1; }

        Token { kind, tag, attrs }
    }
}

// ---------------------------------------------------------------------------
// Entity decoder
// ---------------------------------------------------------------------------

fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut iter = s.char_indices().peekable();
    while let Some((i, c)) = iter.next() {
        if c == '&' {
            let rest = &s[i..];
            if rest.starts_with("&amp;")  { out.push('&'); advance_by(&mut iter, 4); }
            else if rest.starts_with("&lt;")   { out.push('<'); advance_by(&mut iter, 3); }
            else if rest.starts_with("&gt;")   { out.push('>'); advance_by(&mut iter, 3); }
            else if rest.starts_with("&nbsp;") { out.push(' '); advance_by(&mut iter, 5); }
            else if rest.starts_with("&quot;") { out.push('"'); advance_by(&mut iter, 5); }
            else { out.push(c); }
        } else {
            out.push(c);
        }
    }
    out
}

fn advance_by<I: Iterator>(iter: &mut std::iter::Peekable<I>, n: usize) {
    for _ in 0..n { iter.next(); }
}

// ---------------------------------------------------------------------------
// Attribute extractor
// ---------------------------------------------------------------------------

/// Extract the value of a named attribute from a raw attribute string.
fn get_attr<'a>(attrs: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("{}=", key);
    // case-insensitive find
    let lower_attrs = attrs.to_ascii_lowercase();
    let start = lower_attrs.find(needle.as_str())? + needle.len();
    let rest = &attrs[start..];
    if rest.is_empty() { return None; }
    let quote = if rest.starts_with('"') || rest.starts_with('\'') {
        Some(rest.as_bytes()[0] as char)
    } else {
        None
    };
    let val_start = if quote.is_some() { &rest[1..] } else { rest };
    let end = if let Some(q) = quote {
        val_start.find(q).unwrap_or(val_start.len())
    } else {
        val_start
            .find(|c: char| c.is_ascii_whitespace() || c == '>')
            .unwrap_or(val_start.len())
    };
    Some(&val_start[..end])
}

// ---------------------------------------------------------------------------
// Void / skip tag tables
// ---------------------------------------------------------------------------

const VOID_TAGS: &[&str] = &[
    "area","base","br","col","embed","hr","img","input",
    "link","meta","param","source","track","wbr",
];

const SKIP_TAGS: &[&str] = &[
    "head","script","style","link","meta","title",
];

fn is_void(tag: &str) -> bool  { VOID_TAGS.contains(&tag) }
fn is_skip(tag: &str) -> bool  { SKIP_TAGS.contains(&tag) }

// ---------------------------------------------------------------------------
// DOM builder
// ---------------------------------------------------------------------------

/// Parse an HTML string into a tree of `Node`s.
/// Returns the root `#document` element.
pub fn parse(html: &str) -> Node {
    let mut lexer = Lexer::new(html);

    // We use an explicit stack so we avoid recursion limits on deep documents.
    // Each entry is an in-progress Element whose children list we are filling.
    let mut stack: Vec<Element> = vec![Element {
        tag:        "#document".into(),
        id:         String::new(),
        class_name: String::new(),
        style_attr: String::new(),
        style:      Style { display_block: true, ..Default::default() },
        children:   Vec::new(),
    }];

    loop {
        let tok = lexer.next_token();
        match tok.kind {
            TokKind::Eof => break,

            TokKind::Text => {
                if tok.attrs.chars().all(|c| c.is_ascii_whitespace()) {
                    continue; // skip whitespace-only text nodes
                }
                let parent_style = stack.last().unwrap().style.clone();
                let cur = stack.last_mut().unwrap();
                cur.children.push(Node::Text(TextNode {
                    text:  tok.attrs,
                    style: parent_style,
                }));
            }

            TokKind::Open | TokKind::SelfClose => {
                let tag = tok.tag.as_str();

                // Skip non-visual subtrees
                if is_skip(tag) {
                    if !is_void(tag) && tok.kind != TokKind::SelfClose {
                        let mut depth = 1usize;
                        while depth > 0 {
                            let t2 = lexer.next_token();
                            match t2.kind {
                                TokKind::Eof       => break,
                                TokKind::Open      if t2.tag == tok.tag => depth += 1,
                                TokKind::Close     if t2.tag == tok.tag => depth -= 1,
                                _ => {}
                            }
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
                    style:      parent_style,
                    children:   Vec::new(),
                };

                // cascade: inherit → UA defaults → inline
                apply_tag_defaults(&mut el);
                let inline = el.style_attr.clone();
                apply_inline(&inline, &mut el.style);

                if tok.kind == TokKind::SelfClose || is_void(&el.tag) {
                    // Leaf — attach immediately
                    let cur = stack.last_mut().unwrap();
                    cur.children.push(Node::Element(el));
                } else {
                    // Push onto stack to collect children
                    stack.push(el);
                }
            }

            TokKind::Close => {
                // Pop up to the matching open tag
                if let Some(pos) = stack.iter().rposition(|e| e.tag == tok.tag) {
                    // Collect everything above `pos` as nested completions first
                    while stack.len() > pos + 1 {
                        let finished = stack.pop().unwrap();
                        if let Some(parent) = stack.last_mut() {
                            parent.children.push(Node::Element(finished));
                        }
                    }
                    // Now pop the matching element itself
                    if stack.len() > 1 {
                        let finished = stack.pop().unwrap();
                        stack.last_mut().unwrap().children.push(Node::Element(finished));
                    }
                }
            }
        }
    }

    // Drain remaining stack into the document root
    while stack.len() > 1 {
        let finished = stack.pop().unwrap();
        stack.last_mut().unwrap().children.push(Node::Element(finished));
    }

    Node::Element(stack.pop().unwrap())
}

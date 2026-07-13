use super::entities::decode_entities;

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum TokKind { Eof, Text, Open, Close, SelfClose }

pub struct Token {
    pub kind:  TokKind,
    pub tag:   String,
    /// Raw attribute string for element tokens; decoded text for text tokens.
    pub attrs: String,
}

pub struct Lexer<'a> {
    src: &'a [u8],
    pos: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a str) -> Self {
        Lexer { src: src.as_bytes(), pos: 0 }
    }

    fn peek(&self) -> Option<u8> { self.src.get(self.pos).copied() }

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

    pub fn next_token(&mut self) -> Token {
        if self.pos >= self.src.len() {
            return Token { kind: TokKind::Eof, tag: String::new(), attrs: String::new() };
        }

        if self.src[self.pos] != b'<' {
            let start = self.pos;
            while self.pos < self.src.len() && self.src[self.pos] != b'<' {
                self.pos += 1;
            }
            let raw = String::from_utf8_lossy(&self.src[start..self.pos]).into_owned();
            return Token { kind: TokKind::Text, tag: String::new(), attrs: decode_entities(&raw) };
        }

        self.pos += 1; // consume '<'

        if self.src.get(self.pos..self.pos+3) == Some(b"!--") {
            self.pos += 3;
            while self.pos + 2 < self.src.len() {
                if &self.src[self.pos..self.pos+3] == b"-->" { self.pos += 3; break; }
                self.pos += 1;
            }
            return self.next_token();
        }

        if self.peek() == Some(b'!') {
            while self.pos < self.src.len() && self.src[self.pos] != b'>' { self.pos += 1; }
            if self.pos < self.src.len() { self.pos += 1; }
            return self.next_token();
        }

        if self.peek() == Some(b'/') {
            self.pos += 1;
            self.skip_ws();
            let tag = self.read_while(|c| c != b'>' && !c.is_ascii_whitespace())
                          .to_ascii_lowercase();
            while self.pos < self.src.len() && self.src[self.pos] != b'>' { self.pos += 1; }
            if self.pos < self.src.len() { self.pos += 1; }
            return Token { kind: TokKind::Close, tag, attrs: String::new() };
        }

        self.skip_ws();
        let tag = self.read_while(|c| c != b'>' && c != b'/' && !c.is_ascii_whitespace())
                      .to_ascii_lowercase();

        self.skip_ws();
        let mut attrs = String::new();
        let mut in_quote = false;
        let mut qchar = b'"';
        while self.pos < self.src.len() {
            let c = self.src[self.pos];
            if !in_quote && c == b'>' { break; }
            if !in_quote && c == b'/' && self.src.get(self.pos+1) == Some(&b'>') { break; }
            if !in_quote && (c == b'"' || c == b'\'') { in_quote = true;  qchar = c; }
            else if in_quote && c == qchar             { in_quote = false; }
            // Decode the full UTF-8 character at this position instead of
            // casting a single byte — preserves non-ASCII chars in attribute values.
            let src_str = std::str::from_utf8(&self.src[self.pos..]).unwrap_or("");
            let ch = src_str.chars().next().unwrap_or('\u{FFFD}');
            attrs.push(ch);
            self.pos += ch.len_utf8();
        }

        let kind = if self.peek() == Some(b'/') { self.pos += 1; TokKind::SelfClose }
                   else { TokKind::Open };
        if self.peek() == Some(b'>') { self.pos += 1; }

        Token { kind, tag, attrs }
    }

    /// Read raw bytes until `</end_tag>`, returning the decoded string.
    /// Used for `<pre>`, `<textarea>`, etc.
    pub fn read_raw_until(&mut self, end_tag: &str) -> String {
        let mut out = Vec::new();
        let close = format!("</{}>", end_tag).into_bytes();
        while self.pos < self.src.len() {
            if self.pos + close.len() <= self.src.len() {
                let slice = &self.src[self.pos..self.pos+close.len()];
                if slice.eq_ignore_ascii_case(&close) {
                    self.pos += close.len();
                    break;
                }
            }
            out.push(self.src[self.pos]);
            self.pos += 1;
        }
        decode_entities(&String::from_utf8_lossy(&out))
    }
}

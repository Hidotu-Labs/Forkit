/// Tokeniser for the minimal JS interpreter.

#[derive(Debug, PartialEq, Clone)]
pub enum Token {
    Ident(String),
    Str(String),
    Number(f64),
    // Punctuation
    Dot,
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Semi,
    Colon,
    // Assignment
    Eq,        // =
    // Arithmetic
    Plus,      // +
    Minus,     // -
    Star,      // *
    Slash,     // /
    Percent,   // %
    // Comparison  (two-char must be checked before single-char)
    EqEq,      // ==  (also ===)
    BangEq,    // !=  (also !==)
    LtEq,      // <=
    GtEq,      // >=
    Lt,        // <
    Gt,        // >
    // Logical
    AmpAmp,    // &&
    PipePipe,  // ||
    Bang,      // !
    // Increment / decrement
    PlusPlus,  // ++
    MinusMinus,// --
    // Compound assignment
    PlusEq,    // +=
    MinusEq,   // -=
    StarEq,    // *=
    SlashEq,   // /=
    // Special
    Eof,
    Unknown(char),
}

pub struct Lexer {
    pub chars: Vec<char>,
    pub pos:   usize,
}

impl Lexer {
    pub fn new(src: &str) -> Self {
        Lexer { chars: src.chars().collect(), pos: 0 }
    }

    pub fn peek(&self) -> Option<char> { self.chars.get(self.pos).copied() }
    pub fn peek2(&self) -> Option<char> { self.chars.get(self.pos + 1).copied() }

    pub fn bump(&mut self) -> Option<char> {
        let c = self.chars.get(self.pos).copied();
        if c.is_some() { self.pos += 1; }
        c
    }

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            // Whitespace
            while self.peek().map(|c| c.is_ascii_whitespace()).unwrap_or(false) {
                self.bump();
            }
            // Line comment //
            if self.peek() == Some('/') && self.peek2() == Some('/') {
                while self.peek().map(|c| c != '\n').unwrap_or(false) {
                    self.bump();
                }
                continue;
            }
            // Block comment /* ... */
            if self.peek() == Some('/') && self.peek2() == Some('*') {
                self.bump(); self.bump(); // consume /*
                loop {
                    match self.bump() {
                        None => break,
                        Some('*') if self.peek() == Some('/') => { self.bump(); break; }
                        _ => {}
                    }
                }
                continue;
            }
            break;
        }
    }

    pub fn next_token(&mut self) -> Token {
        self.skip_whitespace_and_comments();
        let c = match self.peek() {
            None    => return Token::Eof,
            Some(c) => c,
        };

        // String literals: single or double quotes
        if c == '"' || c == '\'' {
            self.bump();
            let mut s = String::new();
            loop {
                match self.bump() {
                    None | Some('\n') => break,
                    Some(q) if q == c => break,
                    Some('\\') => {
                        match self.bump() {
                            Some('n')  => s.push('\n'),
                            Some('t')  => s.push('\t'),
                            Some('r')  => s.push('\r'),
                            Some('\\') => s.push('\\'),
                            Some('\'') => s.push('\''),
                            Some('"')  => s.push('"'),
                            Some(ch)   => { s.push('\\'); s.push(ch); }
                            None       => break,
                        }
                    }
                    Some(ch) => s.push(ch),
                }
            }
            return Token::Str(s);
        }

        // Template literals — basic support (no interpolation yet)
        if c == '`' {
            self.bump();
            let mut s = String::new();
            loop {
                match self.bump() {
                    None | Some('`') => break,
                    Some('\\') => {
                        match self.bump() {
                            Some('n')  => s.push('\n'),
                            Some('t')  => s.push('\t'),
                            Some(ch)   => s.push(ch),
                            None       => break,
                        }
                    }
                    Some(ch) => s.push(ch),
                }
            }
            return Token::Str(s);
        }

        // Number literals
        if c.is_ascii_digit() {
            // hex: 0x…
            if c == '0' && self.peek2().map(|d| d == 'x' || d == 'X').unwrap_or(false) {
                self.bump(); self.bump(); // consume 0x
                let mut hex = String::new();
                while self.peek().map(|d| d.is_ascii_hexdigit()).unwrap_or(false) {
                    hex.push(self.bump().unwrap());
                }
                let n = i64::from_str_radix(&hex, 16).unwrap_or(0) as f64;
                return Token::Number(n);
            }
            let mut num_str = String::new();
            while self.peek().map(|d| d.is_ascii_digit() || d == '.').unwrap_or(false) {
                num_str.push(self.bump().unwrap());
            }
            return Token::Number(num_str.parse::<f64>().unwrap_or(0.0));
        }

        // Identifiers / keywords
        if c.is_alphabetic() || c == '_' || c == '$' {
            let mut ident = String::new();
            while self.peek().map(|d| d.is_alphanumeric() || d == '_' || d == '$').unwrap_or(false) {
                ident.push(self.bump().unwrap());
            }
            return Token::Ident(ident);
        }

        // Two-char and one-char operators
        self.bump(); // consume first char
        match c {
            '=' => {
                if self.peek() == Some('=') {
                    self.bump();
                    if self.peek() == Some('=') { self.bump(); } // ===
                    Token::EqEq
                } else {
                    Token::Eq
                }
            }
            '!' => {
                if self.peek() == Some('=') {
                    self.bump();
                    if self.peek() == Some('=') { self.bump(); } // !==
                    Token::BangEq
                } else {
                    Token::Bang
                }
            }
            '<' => {
                if self.peek() == Some('=') { self.bump(); Token::LtEq }
                else { Token::Lt }
            }
            '>' => {
                if self.peek() == Some('=') { self.bump(); Token::GtEq }
                else { Token::Gt }
            }
            '&' => {
                if self.peek() == Some('&') { self.bump(); Token::AmpAmp }
                else { Token::Unknown('&') }
            }
            '|' => {
                if self.peek() == Some('|') { self.bump(); Token::PipePipe }
                else { Token::Unknown('|') }
            }
            '+' => {
                if self.peek() == Some('=') { self.bump(); Token::PlusEq }
                else if self.peek() == Some('+') { self.bump(); Token::PlusPlus }
                else { Token::Plus }
            }
            '-' => {
                if self.peek() == Some('=') { self.bump(); Token::MinusEq }
                else if self.peek() == Some('-') { self.bump(); Token::MinusMinus }
                else { Token::Minus }
            }
            '*' => {
                if self.peek() == Some('=') { self.bump(); Token::StarEq }
                else if self.peek() == Some('*') { self.bump(); Token::Star } // **
                else { Token::Star }
            }
            '/' => {
                if self.peek() == Some('=') { self.bump(); Token::SlashEq }
                else { Token::Slash }
            }
            '%' => Token::Percent,
            '.' => Token::Dot,
            '(' => Token::LParen,
            ')' => Token::RParen,
            '{' => Token::LBrace,
            '}' => Token::RBrace,
            '[' => Token::LBracket,
            ']' => Token::RBracket,
            ',' => Token::Comma,
            ';' => Token::Semi,
            ':' => Token::Colon,
            _   => Token::Unknown(c),
        }
    }

    /// Peek at the next token without consuming.
    pub fn peek_token(&mut self) -> Token {
        let saved = self.pos;
        let tok = self.next_token();
        self.pos = saved;
        tok
    }
}

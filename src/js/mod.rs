/// Minimal JavaScript interpreter.
/// Supports: variables (let/const/var), arithmetic, comparisons, logical ops,
/// string concatenation, and console.log / console.warn.

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub enum JsValue {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    Str(String),
}

impl JsValue {
    /// Coerce to a display string (JS `String(value)` semantics).
    pub fn to_display(&self) -> String {
        match self {
            JsValue::Undefined  => "undefined".to_owned(),
            JsValue::Null       => "null".to_owned(),
            JsValue::Bool(b)    => b.to_string(),
            JsValue::Number(n)  => {
                if n.is_nan()              { return "NaN".to_owned(); }
                if *n == f64::INFINITY     { return "Infinity".to_owned(); }
                if *n == f64::NEG_INFINITY { return "-Infinity".to_owned(); }
                if n.fract() == 0.0 && n.abs() < 1e15 {
                    format!("{}", *n as i64)
                } else {
                    format!("{}", n)
                }
            }
            JsValue::Str(s) => s.clone(),
        }
    }

    /// Coerce to f64 (JS `Number(value)` semantics).
    fn to_number(&self) -> f64 {
        match self {
            JsValue::Number(n)  => *n,
            JsValue::Bool(b)    => if *b { 1.0 } else { 0.0 },
            JsValue::Str(s)     => s.trim().parse::<f64>().unwrap_or(f64::NAN),
            JsValue::Null       => 0.0,
            JsValue::Undefined  => f64::NAN,
        }
    }

    /// Coerce to bool (JS truthy semantics).
    fn to_bool(&self) -> bool {
        match self {
            JsValue::Bool(b)   => *b,
            JsValue::Number(n) => *n != 0.0 && !n.is_nan(),
            JsValue::Str(s)    => !s.is_empty(),
            JsValue::Null      => false,
            JsValue::Undefined => false,
        }
    }
}

struct Scope {
    vars: HashMap<String, JsValue>,
}

impl Scope {
    fn new() -> Self {
        Scope { vars: HashMap::new() }
    }

    fn set(&mut self, name: &str, val: JsValue) {
        self.vars.insert(name.to_owned(), val);
    }

    fn get(&self, name: &str) -> JsValue {
        self.vars.get(name).cloned().unwrap_or(JsValue::Undefined)
    }
}

#[derive(Debug, PartialEq, Clone)]
enum Token {
    Ident(String),
    Str(String),
    Number(f64),
    Dot,
    LParen,
    RParen,
    Comma,
    Semi,
    Eq,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    EqEq,
    BangEq,
    LtEq,
    GtEq,
    Lt,
    Gt,
    AmpAmp,
    PipePipe,
    Bang,
    PlusEq,
    MinusEq,
    StarEq,
    SlashEq,
    Eof,
    Unknown(char),
}

struct Lexer {
    chars:  Vec<char>,
    pos:    usize,
    peeked: Option<Token>,
}

impl Lexer {
    fn new(src: &str) -> Self {
        Lexer { chars: src.chars().collect(), pos: 0, peeked: None }
    }

    fn peek(&self) -> Option<char> { self.chars.get(self.pos).copied() }
    fn peek2(&self) -> Option<char> { self.chars.get(self.pos + 1).copied() }

    fn bump(&mut self) -> Option<char> {
        let c = self.chars.get(self.pos).copied();
        if c.is_some() { self.pos += 1; }
        c
    }

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            while self.peek().map(|c| c.is_ascii_whitespace()).unwrap_or(false) {
                self.bump();
            }
            if self.peek() == Some('/') && self.peek2() == Some('/') {
                while self.peek().map(|c| c != '\n').unwrap_or(false) {
                    self.bump();
                }
                continue;
            }
            if self.peek() == Some('/') && self.peek2() == Some('*') {
                self.bump(); self.bump();
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

    fn next_token(&mut self) -> Token {
        if let Some(t) = self.peeked.take() {
            return t;
        }
        self.lex_next()
    }

    fn peek_token(&mut self) -> Token {
        if self.peeked.is_none() {
            self.peeked = Some(self.lex_next());
        }
        self.peeked.clone().unwrap()
    }

    fn lex_next(&mut self) -> Token {
        self.skip_whitespace_and_comments();
        let c = match self.peek() {
            None    => return Token::Eof,
            Some(c) => c,
        };

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

        if c.is_ascii_digit() {
            let mut num_str = String::new();
            if c == '0' && self.peek2().map(|d| d == 'x' || d == 'X').unwrap_or(false) {
                self.bump(); self.bump(); // consume 0x
                let mut hex = String::new();
                while self.peek().map(|d| d.is_ascii_hexdigit()).unwrap_or(false) {
                    hex.push(self.bump().unwrap());
                }
                let n = i64::from_str_radix(&hex, 16).unwrap_or(0) as f64;
                return Token::Number(n);
            }
            while self.peek().map(|d| d.is_ascii_digit() || d == '.').unwrap_or(false) {
                num_str.push(self.bump().unwrap());
            }
            return Token::Number(num_str.parse::<f64>().unwrap_or(0.0));
        }

        if c.is_alphabetic() || c == '_' || c == '$' {
            let mut ident = String::new();
            while self.peek().map(|d| d.is_alphanumeric() || d == '_' || d == '$').unwrap_or(false) {
                ident.push(self.bump().unwrap());
            }
            return Token::Ident(ident);
        }

        self.bump();
        match c {
            '=' => {
                if self.peek() == Some('=') {
                    self.bump();
                    Token::EqEq
                } else {
                    Token::Eq
                }
            }
            '!' => {
                if self.peek() == Some('=') {
                    self.bump();
                    if self.peek() == Some('=') { self.bump(); }
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
                else if self.peek() == Some('+') { self.bump(); Token::Plus }
                else { Token::Plus }
            }
            '-' => {
                if self.peek() == Some('=') { self.bump(); Token::MinusEq }
                else if self.peek() == Some('-') { self.bump(); Token::Minus }
                else { Token::Minus }
            }
            '*' => {
                if self.peek() == Some('=') { self.bump(); Token::StarEq }
                else if self.peek() == Some('*') { self.bump(); Token::Star }
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
            ',' => Token::Comma,
            ';' => Token::Semi,
            _   => Token::Unknown(c),
        }
    }
}

fn eval_expr(lexer: &mut Lexer, scope: &Scope) -> JsValue {
    eval_or(lexer, scope)
}

fn eval_or(lexer: &mut Lexer, scope: &Scope) -> JsValue {
    let mut val = eval_and(lexer, scope);
    while lexer.peek_token() == Token::PipePipe {
        lexer.next_token();
        if val.to_bool() {
            skip_and_expr(lexer);
        } else {
            val = eval_and(lexer, scope);
        }
    }
    val
}

fn eval_and(lexer: &mut Lexer, scope: &Scope) -> JsValue {
    let mut val = eval_equality(lexer, scope);
    while lexer.peek_token() == Token::AmpAmp {
        lexer.next_token();
        if !val.to_bool() {
            skip_equality_expr(lexer);
        } else {
            val = eval_equality(lexer, scope);
        }
    }
    val
}

fn eval_equality(lexer: &mut Lexer, scope: &Scope) -> JsValue {
    let mut val = eval_relational(lexer, scope);
    loop {
        match lexer.peek_token() {
            Token::EqEq => {
                lexer.next_token();
                let rhs = eval_relational(lexer, scope);
                val = JsValue::Bool(js_loose_eq(&val, &rhs));
            }
            Token::BangEq => {
                lexer.next_token();
                let rhs = eval_relational(lexer, scope);
                val = JsValue::Bool(!js_loose_eq(&val, &rhs));
            }
            _ => break,
        }
    }
    val
}

fn eval_relational(lexer: &mut Lexer, scope: &Scope) -> JsValue {
    let mut val = eval_additive(lexer, scope);
    loop {
        match lexer.peek_token() {
            Token::Lt => {
                lexer.next_token();
                let rhs = eval_additive(lexer, scope);
                val = JsValue::Bool(val.to_number() < rhs.to_number());
            }
            Token::Gt => {
                lexer.next_token();
                let rhs = eval_additive(lexer, scope);
                val = JsValue::Bool(val.to_number() > rhs.to_number());
            }
            Token::LtEq => {
                lexer.next_token();
                let rhs = eval_additive(lexer, scope);
                val = JsValue::Bool(val.to_number() <= rhs.to_number());
            }
            Token::GtEq => {
                lexer.next_token();
                let rhs = eval_additive(lexer, scope);
                val = JsValue::Bool(val.to_number() >= rhs.to_number());
            }
            _ => break,
        }
    }
    val
}

fn eval_additive(lexer: &mut Lexer, scope: &Scope) -> JsValue {
    let mut val = eval_multiplicative(lexer, scope);
    loop {
        match lexer.peek_token() {
            Token::Plus => {
                lexer.next_token();
                let rhs = eval_multiplicative(lexer, scope);
                val = match (&val, &rhs) {
                    (JsValue::Number(a), JsValue::Number(b)) => JsValue::Number(a + b),
                    _ => JsValue::Str(format!("{}{}", val.to_display(), rhs.to_display())),
                };
            }
            Token::Minus => {
                lexer.next_token();
                let rhs = eval_multiplicative(lexer, scope);
                val = JsValue::Number(val.to_number() - rhs.to_number());
            }
            _ => break,
        }
    }
    val
}

fn eval_multiplicative(lexer: &mut Lexer, scope: &Scope) -> JsValue {
    let mut val = eval_unary(lexer, scope);
    loop {
        match lexer.peek_token() {
            Token::Star => {
                lexer.next_token();
                let rhs = eval_unary(lexer, scope);
                val = JsValue::Number(val.to_number() * rhs.to_number());
            }
            Token::Slash => {
                lexer.next_token();
                let rhs = eval_unary(lexer, scope);
                val = JsValue::Number(val.to_number() / rhs.to_number());
            }
            Token::Percent => {
                lexer.next_token();
                let rhs = eval_unary(lexer, scope);
                val = JsValue::Number(val.to_number() % rhs.to_number());
            }
            _ => break,
        }
    }
    val
}

fn eval_unary(lexer: &mut Lexer, scope: &Scope) -> JsValue {
    match lexer.peek_token() {
        Token::Bang => {
            lexer.next_token();
            let val = eval_unary(lexer, scope);
            JsValue::Bool(!val.to_bool())
        }
        Token::Minus => {
            lexer.next_token();
            let val = eval_unary(lexer, scope);
            JsValue::Number(-val.to_number())
        }
        _ => eval_primary(lexer, scope),
    }
}

fn eval_primary(lexer: &mut Lexer, scope: &Scope) -> JsValue {
    match lexer.next_token() {
        Token::Str(s)    => JsValue::Str(s),
        Token::Number(n) => JsValue::Number(n),
        Token::Ident(id) => match id.as_str() {
            "true"      => JsValue::Bool(true),
            "false"     => JsValue::Bool(false),
            "null"      => JsValue::Null,
            "undefined" => JsValue::Undefined,
            "NaN"       => JsValue::Number(f64::NAN),
            "Infinity"  => JsValue::Number(f64::INFINITY),
            // Look up in scope; for now method calls on objects just return Undefined
            name => {
                // Check for a dot-call like `foo.bar(...)` — consume and ignore
                if lexer.peek_token() == Token::Dot {
                    // skip the whole chain (already consumed the object name)
                    skip_dot_call_chain(lexer);
                    return JsValue::Undefined;
                }
                scope.get(name)
            }
        },
        Token::LParen => {
            let inner = eval_expr(lexer, scope);
            if lexer.peek_token() == Token::RParen {
                lexer.next_token();
            }
            inner
        }
        _ => JsValue::Undefined,
    }
}

fn js_loose_eq(a: &JsValue, b: &JsValue) -> bool {
    match (a, b) {
        (JsValue::Null,      JsValue::Null)      => true,
        (JsValue::Null,      JsValue::Undefined) => true,
        (JsValue::Undefined, JsValue::Null)      => true,
        (JsValue::Undefined, JsValue::Undefined) => true,
        (JsValue::Bool(x),   JsValue::Bool(y))   => x == y,
        (JsValue::Number(x), JsValue::Number(y)) => x == y,
        (JsValue::Str(x),    JsValue::Str(y))    => x == y,
        _ => a.to_number() == b.to_number(),
    }
}

fn skip_and_expr(lexer: &mut Lexer) {
    skip_expr_at_precedence(lexer, Prec::And);
}

/// Skip one "equality-level" expression (used by short-circuit AND).
fn skip_equality_expr(lexer: &mut Lexer) {
    skip_expr_at_precedence(lexer, Prec::Equality);
}

enum Prec { And, Equality }

fn skip_expr_at_precedence(lexer: &mut Lexer, _prec: Prec) {
    let mut depth = 0i32;
    loop {
        match lexer.peek_token() {
            Token::Eof | Token::Semi | Token::Comma => break,
            Token::RParen if depth == 0 => break,
            Token::PipePipe if depth == 0 => break,
            Token::LParen => { lexer.next_token(); depth += 1; }
            Token::RParen => { lexer.next_token(); depth -= 1; }
            _ => { lexer.next_token(); }
        }
    }
}

fn skip_dot_call_chain(lexer: &mut Lexer) {
    loop {
        if lexer.peek_token() != Token::Dot { break; }
        lexer.next_token();
        if let Token::Ident(_) = lexer.peek_token() { lexer.next_token(); }
        if lexer.peek_token() == Token::LParen {
            lexer.next_token();
            let mut depth = 1;
            loop {
                match lexer.next_token() {
                    Token::Eof => break,
                    Token::LParen => depth += 1,
                    Token::RParen => { depth -= 1; if depth == 0 { break; } }
                    _ => {}
                }
            }
        }
    }
}

fn run_statement(lexer: &mut Lexer, scope: &mut Scope, entries: &mut Vec<ConsoleEntry>) {
    match lexer.peek_token() {
        Token::Ident(ref id) if matches!(id.as_str(), "var" | "let" | "const") => {
            lexer.next_token();

            let name = match lexer.next_token() {
                Token::Ident(n) => n,
                _ => { skip_to_semi(lexer); return; }
            };

            let val = if lexer.peek_token() == Token::Eq {
                lexer.next_token(); // consume `=`
                eval_expr(lexer, scope)
            } else {
                JsValue::Undefined
            };

            scope.set(&name, val);

            if lexer.peek_token() == Token::Semi { lexer.next_token(); }
            return;
        }

        Token::Ident(_) => {
            let saved = lexer.pos;
            let name = match lexer.next_token() {
                Token::Ident(n) => n,
                _ => { lexer.pos = saved; skip_to_semi(lexer); return; }
            };

            match lexer.peek_token() {
                Token::Eq => {
                    lexer.next_token();
                    let val = eval_expr(lexer, scope);
                    scope.set(&name, val);
                    if lexer.peek_token() == Token::Semi { lexer.next_token(); }
                    return;
                }
                Token::PlusEq | Token::MinusEq | Token::StarEq | Token::SlashEq => {
                    let op = lexer.next_token();
                    let rhs = eval_expr(lexer, scope);
                    let lhs = scope.get(&name);
                    let result = match op {
                        Token::PlusEq  => match (&lhs, &rhs) {
                            (JsValue::Number(a), JsValue::Number(b)) => JsValue::Number(a + b),
                            _ => JsValue::Str(format!("{}{}", lhs.to_display(), rhs.to_display())),
                        },
                        Token::MinusEq => JsValue::Number(lhs.to_number() - rhs.to_number()),
                        Token::StarEq  => JsValue::Number(lhs.to_number() * rhs.to_number()),
                        Token::SlashEq => JsValue::Number(lhs.to_number() / rhs.to_number()),
                        _ => unreachable!(),
                    };
                    scope.set(&name, result);
                    if lexer.peek_token() == Token::Semi { lexer.next_token(); }
                    return;
                }
                Token::Dot => {
                    lexer.pos = saved;
                }
                _ => {
                    lexer.pos = saved;
                    skip_to_semi(lexer);
                    return;
                }
            }
        }

        _ => {}
    }

    if lexer.peek_token() != Token::Ident("console".to_owned()) {
        skip_to_semi(lexer);
        return;
    }

    lexer.next_token(); // consume `console`

    if lexer.next_token() != Token::Dot { return; }

    let method = match lexer.next_token() {
        Token::Ident(m) => m,
        _ => return,
    };

    if lexer.next_token() != Token::LParen { return; }

    let mut args: Vec<JsValue> = Vec::new();
    loop {
        if lexer.peek_token() == Token::RParen || lexer.peek_token() == Token::Eof {
            break;
        }
        args.push(eval_expr(lexer, scope));
        if lexer.peek_token() == Token::Comma {
            lexer.next_token();
        } else {
            break;
        }
    }

    if lexer.peek_token() == Token::RParen { lexer.next_token(); }
    if lexer.peek_token() == Token::Semi   { lexer.next_token(); }

    let message = args.iter().map(|v| v.to_display()).collect::<Vec<_>>().join(" ");

    match method.as_str() {
        "log"  => entries.push(ConsoleEntry { level: ConsoleLevel::Log,  message }),
        "warn" => entries.push(ConsoleEntry { level: ConsoleLevel::Warn, message }),
        _      => {}
    }
}

/// Skip tokens up to and including the next `;` or `Eof`.
fn skip_to_semi(lexer: &mut Lexer) {
    loop {
        match lexer.next_token() {
            Token::Semi | Token::Eof => break,
            _ => {}
        }
    }
}

#[derive(Debug, Clone)]
pub enum ConsoleLevel {
    Log,
    Warn,
}

/// A single console output entry produced by script execution.
#[derive(Debug, Clone)]
pub struct ConsoleEntry {
    pub level:   ConsoleLevel,
    pub message: String,
}

pub fn execute(src: &str) -> Vec<ConsoleEntry> {
    let mut entries = Vec::new();
    let mut lexer   = Lexer::new(src);
    let mut scope   = Scope::new();
    loop {
        while lexer.peek_token() == Token::Semi {
            lexer.next_token();
        }
        if lexer.peek_token() == Token::Eof { break; }
        run_statement(&mut lexer, &mut scope, &mut entries);
    }
    entries
}

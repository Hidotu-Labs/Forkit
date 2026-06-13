/// Statement runner — drives the top-level execution loop.

use crate::js::lexer::{Lexer, Token};
use crate::js::scope::Scope;
use crate::js::types::JsValue;
use crate::js::console::{ConsoleEntry, ConsoleLevel};
use crate::js::dom::{JsDom, JsElement};

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

/// Execute JS without any DOM access (original behaviour).
pub fn execute(src: &str) -> Vec<ConsoleEntry> {
    execute_inner(src, None)
}

/// Execute JS with read access to the parsed DOM.
pub fn execute_with_dom<'a>(src: &str, dom: &'a JsDom<'a>) -> Vec<ConsoleEntry> {
    execute_inner(src, Some(dom))
}

fn execute_inner(src: &str, dom: Option<&JsDom<'_>>) -> Vec<ConsoleEntry> {
    let mut entries = Vec::new();
    let mut lexer   = Lexer::new(src);
    let mut scope   = Scope::new();
    loop {
        // Skip bare semicolons and lone closing braces (end of unknown blocks).
        loop {
            match lexer.peek_token() {
                Token::Semi | Token::RBrace => { lexer.next_token(); }
                _ => break,
            }
        }
        if lexer.peek_token() == Token::Eof { break; }

        let pos_before = lexer.pos;
        run_statement(&mut lexer, &mut scope, &mut entries, dom);

        // Safety: if run_statement consumed nothing, forcibly advance one token
        // to prevent an infinite loop on any token we don't handle.
        if lexer.pos == pos_before {
            lexer.next_token();
        }
    }
    entries
}

// ---------------------------------------------------------------------------
// Statement dispatch
// ---------------------------------------------------------------------------

fn run_statement(
    lexer:   &mut Lexer,
    scope:   &mut Scope,
    entries: &mut Vec<ConsoleEntry>,
    dom:     Option<&JsDom<'_>>,
) {
    match lexer.peek_token() {
        // ── Variable declaration: let / const / var ──────────────────────────
        Token::Ident(ref id) if matches!(id.as_str(), "var" | "let" | "const") => {
            lexer.next_token(); // consume keyword
            let name = match lexer.next_token() {
                Token::Ident(n) => n,
                _ => { skip_statement(lexer); return; }
            };
            // Handle destructuring: let { a, b } = ... or let [ a ] = ...
            if matches!(name.as_str(), "") {
                skip_statement(lexer);
                return;
            }
            let val = if lexer.peek_token() == Token::Eq {
                lexer.next_token();
                eval_expr_with_dom(lexer, scope, dom)
            } else {
                JsValue::Undefined
            };
            scope.set(&name, val);
            if lexer.peek_token() == Token::Semi { lexer.next_token(); }
            return;
        }

        // ── Keywords that introduce blocks we can't execute — skip them ───────
        Token::Ident(ref id) if matches!(id.as_str(),
            "if" | "else" | "for" | "while" | "do" | "switch" |
            "function" | "class" | "return" | "throw" | "try" |
            "catch" | "finally" | "import" | "export" | "async" |
            "await" | "yield" | "delete" | "typeof" | "void" |
            "new" | "break" | "continue" | "debugger" | "with"
        ) => {
            skip_statement(lexer);
            return;
        }

        // ── Bare block: { ... } ───────────────────────────────────────────────
        Token::LBrace => {
            skip_block(lexer);
            return;
        }

        // ── Identifier-starting statement ────────────────────────────────────
        Token::Ident(_) => {
            let saved = lexer.pos;
            let name = match lexer.next_token() {
                Token::Ident(n) => n,
                _ => { lexer.pos = saved; skip_statement(lexer); return; }
            };

            match lexer.peek_token() {
                // Simple assignment: name = expr
                Token::Eq => {
                    lexer.next_token();
                    let val = eval_expr_with_dom(lexer, scope, dom);
                    scope.set(&name, val);
                    if lexer.peek_token() == Token::Semi { lexer.next_token(); }
                    return;
                }
                // Compound assignment
                Token::PlusEq | Token::MinusEq | Token::StarEq | Token::SlashEq => {
                    let op  = lexer.next_token();
                    let rhs = eval_expr_with_dom(lexer, scope, dom);
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
                // dot-call — restore and fall through to console/document handler
                Token::Dot => { lexer.pos = saved; }
                // Anything else (e.g. `foo(...)` call, `foo++`, etc.) — skip
                _ => { lexer.pos = saved; skip_statement(lexer); return; }
            }
        }

        _ => {
            // Unknown token at statement level — skip it
            skip_statement(lexer);
            return;
        }
    }

    // ── Dispatch on the leading identifier (console / document) ──────────────
    let leading = match lexer.peek_token() {
        Token::Ident(s) => s,
        _ => { skip_statement(lexer); return; }
    };

    match leading.as_str() {
        "console"  => handle_console(lexer, scope, entries, dom),
        "document" => { handle_document_stmt(lexer, scope, entries, dom); }
        _          => { skip_statement(lexer); }
    }
}

// ---------------------------------------------------------------------------
// console.log / .warn / .error
// ---------------------------------------------------------------------------

fn handle_console(
    lexer:   &mut Lexer,
    scope:   &mut Scope,
    entries: &mut Vec<ConsoleEntry>,
    dom:     Option<&JsDom<'_>>,
) {
    lexer.next_token(); // consume `console`
    if lexer.next_token() != Token::Dot { return; }

    let method = match lexer.next_token() {
        Token::Ident(m) => m,
        _ => return,
    };

    if lexer.next_token() != Token::LParen { return; }

    let mut args: Vec<JsValue> = Vec::new();
    loop {
        if matches!(lexer.peek_token(), Token::RParen | Token::Eof) { break; }
        args.push(eval_expr_with_dom(lexer, scope, dom));
        if lexer.peek_token() == Token::Comma { lexer.next_token(); } else { break; }
    }
    if lexer.peek_token() == Token::RParen { lexer.next_token(); }
    if lexer.peek_token() == Token::Semi   { lexer.next_token(); }

    let message = args.iter().map(|v| v.to_display()).collect::<Vec<_>>().join(" ");
    match method.as_str() {
        "log"   => entries.push(ConsoleEntry { level: ConsoleLevel::Log,   message }),
        "warn"  => entries.push(ConsoleEntry { level: ConsoleLevel::Warn,  message }),
        "error" => entries.push(ConsoleEntry { level: ConsoleLevel::Error, message }),
        _       => {}
    }
}

// ---------------------------------------------------------------------------
// document.* statement (bare, result discarded)
// ---------------------------------------------------------------------------

fn handle_document_stmt(
    lexer:   &mut Lexer,
    scope:   &mut Scope,
    _entries: &mut Vec<ConsoleEntry>,
    dom:     Option<&JsDom<'_>>,
) {
    eval_document_expr(lexer, scope, dom);
    if lexer.peek_token() == Token::Semi { lexer.next_token(); }
}

// ---------------------------------------------------------------------------
// document.* expression evaluator
// ---------------------------------------------------------------------------

pub fn eval_document_expr(
    lexer: &mut Lexer,
    scope: &mut Scope,
    dom:   Option<&JsDom<'_>>,
) -> JsValue {
    if lexer.peek_token() != Token::Dot {
        return JsValue::Undefined;
    }
    lexer.next_token(); // consume `.`

    let method = match lexer.next_token() {
        Token::Ident(m) => m,
        _ => return JsValue::Undefined,
    };

    // document.title — property, no parens
    if method == "title" {
        let val = dom.map(|d| JsValue::Str(d.title())).unwrap_or(JsValue::Str(String::new()));
        return chain_string_props(lexer, val);
    }

    // All other document methods require ()
    if lexer.next_token() != Token::LParen {
        return JsValue::Undefined;
    }

    let arg = match lexer.peek_token() {
        Token::RParen | Token::Eof => String::new(),
        _ => {
            let v = eval_expr_with_dom(lexer, scope, dom);
            v.to_display()
        }
    };
    // swallow any extra args
    while !matches!(lexer.peek_token(), Token::RParen | Token::Eof) {
        lexer.next_token();
    }
    if lexer.peek_token() == Token::RParen { lexer.next_token(); }

    match method.as_str() {
        "getElementById" => {
            let el = dom.and_then(|d| d.get_element_by_id(&arg));
            let val = element_to_value(el);
            chain_element_props(lexer, val, dom)
        }
        "querySelector" => {
            let el = dom.and_then(|d| d.query_selector(&arg));
            let val = element_to_value(el);
            chain_element_props(lexer, val, dom)
        }
        "getElementsByTagName" => {
            let els = dom.map(|d| d.get_elements_by_tag_name(&arg)).unwrap_or_default();
            node_list_value(els)
        }
        "getElementsByClassName" => {
            let els = dom.map(|d| d.get_elements_by_class_name(&arg)).unwrap_or_default();
            node_list_value(els)
        }
        "querySelectorAll" => {
            let els = dom.map(|d| d.query_selector_all(&arg)).unwrap_or_default();
            node_list_value(els)
        }
        _ => JsValue::Undefined,
    }
}

// ---------------------------------------------------------------------------
// Element property chain
// ---------------------------------------------------------------------------

fn chain_element_props(
    lexer: &mut Lexer,
    val:   JsValue,
    _dom:  Option<&JsDom<'_>>,
) -> JsValue {
    chain_props_inner(lexer, val)
}

fn chain_props_inner(lexer: &mut Lexer, val: JsValue) -> JsValue {
    if lexer.peek_token() != Token::Dot { return val; }
    lexer.next_token(); // consume `.`

    let prop = match lexer.next_token() {
        Token::Ident(p) => p,
        _ => return val,
    };

    let next_val = if let JsValue::Str(ref s) = val {
        if s.starts_with("\x00elem\x00") {
            read_element_prop(s, &prop, lexer)
        } else if s.starts_with("\x00list\x00") {
            read_list_prop(s, &prop)
        } else {
            read_string_prop(s, &prop)
        }
    } else {
        JsValue::Undefined
    };

    chain_props_inner(lexer, next_val)
}

fn read_element_prop(encoded: &str, prop: &str, lexer: &mut Lexer) -> JsValue {
    let el = decode_element(encoded);
    match prop {
        "id"          => JsValue::Str(el.id),
        "className"   => JsValue::Str(el.class_name),
        "tagName"     => JsValue::Str(el.tag.to_ascii_uppercase()),
        "textContent" => JsValue::Str(el.text_content),
        "innerHTML"   => JsValue::Str(el.inner_html),
        "innerText"   => JsValue::Str(el.text_content),
        "length"      => JsValue::Number(1.0),
        "children"    => JsValue::Number(el.children.len() as f64),
        "getAttribute" => {
            if lexer.peek_token() == Token::LParen {
                lexer.next_token();
                let name = match lexer.peek_token() {
                    Token::Str(s)   => { lexer.next_token(); s }
                    Token::Ident(s) => { lexer.next_token(); s }
                    _ => String::new(),
                };
                if lexer.peek_token() == Token::RParen { lexer.next_token(); }
                el.get_attribute(&name).map(JsValue::Str).unwrap_or(JsValue::Null)
            } else {
                JsValue::Undefined
            }
        }
        _ => JsValue::Undefined,
    }
}

fn read_string_prop(s: &str, prop: &str) -> JsValue {
    match prop {
        "length" => JsValue::Number(s.chars().count() as f64),
        _        => JsValue::Undefined,
    }
}

fn read_list_prop(s: &str, prop: &str) -> JsValue {
    let parts: Vec<&str> = s.splitn(4, '\x00').collect();
    let count: f64 = parts.get(2).and_then(|c| c.parse().ok()).unwrap_or(0.0);
    match prop {
        "length" => JsValue::Number(count),
        _        => JsValue::Undefined,
    }
}

fn chain_string_props(lexer: &mut Lexer, val: JsValue) -> JsValue {
    chain_props_inner(lexer, val)
}

// ---------------------------------------------------------------------------
// NodeList encoding
// ---------------------------------------------------------------------------

fn node_list_value(els: Vec<JsElement>) -> JsValue {
    let count = els.len();
    let mut s = format!("\x00list\x00{}\x00", count);
    for (i, el) in els.iter().enumerate() {
        if i > 0 { s.push('\x01'); }
        s.push_str(&encode_element(el));
    }
    JsValue::Str(s)
}

// ---------------------------------------------------------------------------
// Element encoding/decoding
// ---------------------------------------------------------------------------

fn encode_element(el: &JsElement) -> String {
    format!(
        "\x00elem\x00{}\x00{}\x00{}\x00{}\x00{}\x00{}\x00{}",
        escape_field(&el.tag),
        escape_field(&el.id),
        escape_field(&el.class_name),
        escape_field(&el.text_content),
        escape_field(&el.inner_html),
        escape_field(&el.attrs_raw),
        el.children.len(),
    )
}

fn element_to_value(el: Option<JsElement>) -> JsValue {
    match el {
        Some(e) => JsValue::Str(encode_element(&e)),
        None    => JsValue::Null,
    }
}

fn decode_element(s: &str) -> JsElement {
    let parts: Vec<&str> = s.split('\x00').collect();
    let get = |i: usize| parts.get(i).copied().unwrap_or("").to_owned();
    JsElement {
        tag:          unescape_field(&get(2)),
        id:           unescape_field(&get(3)),
        class_name:   unescape_field(&get(4)),
        text_content: unescape_field(&get(5)),
        inner_html:   unescape_field(&get(6)),
        attrs_raw:    unescape_field(&get(7)),
        children:     Vec::new(),
    }
}

fn escape_field(s: &str) -> String {
    s.replace('\x00', "\x02").replace('\x01', "\x03")
}

fn unescape_field(s: &str) -> String {
    s.replace('\x02', "\x00").replace('\x03', "\x01")
}

// ---------------------------------------------------------------------------
// DOM-aware expression evaluator
// ---------------------------------------------------------------------------

pub fn eval_expr_with_dom(
    lexer: &mut Lexer,
    scope: &mut Scope,
    dom:   Option<&JsDom<'_>>,
) -> JsValue {
    eval_or_dom(lexer, scope, dom)
}

fn eval_or_dom(lexer: &mut Lexer, scope: &mut Scope, dom: Option<&JsDom<'_>>) -> JsValue {
    let mut val = eval_and_dom(lexer, scope, dom);
    while lexer.peek_token() == Token::PipePipe {
        lexer.next_token();
        if val.to_bool() {
            crate::js::eval::skip_and_expr(lexer);
        } else {
            val = eval_and_dom(lexer, scope, dom);
        }
    }
    val
}

fn eval_and_dom(lexer: &mut Lexer, scope: &mut Scope, dom: Option<&JsDom<'_>>) -> JsValue {
    let mut val = eval_equality_dom(lexer, scope, dom);
    while lexer.peek_token() == Token::AmpAmp {
        lexer.next_token();
        if !val.to_bool() {
            crate::js::eval::skip_equality_expr(lexer);
        } else {
            val = eval_equality_dom(lexer, scope, dom);
        }
    }
    val
}

fn eval_equality_dom(lexer: &mut Lexer, scope: &mut Scope, dom: Option<&JsDom<'_>>) -> JsValue {
    let mut val = eval_relational_dom(lexer, scope, dom);
    loop {
        match lexer.peek_token() {
            Token::EqEq => {
                lexer.next_token();
                let rhs = eval_relational_dom(lexer, scope, dom);
                val = JsValue::Bool(crate::js::eval::js_loose_eq(&val, &rhs));
            }
            Token::BangEq => {
                lexer.next_token();
                let rhs = eval_relational_dom(lexer, scope, dom);
                val = JsValue::Bool(!crate::js::eval::js_loose_eq(&val, &rhs));
            }
            _ => break,
        }
    }
    val
}

fn eval_relational_dom(lexer: &mut Lexer, scope: &mut Scope, dom: Option<&JsDom<'_>>) -> JsValue {
    let mut val = eval_additive_dom(lexer, scope, dom);
    loop {
        match lexer.peek_token() {
            Token::Lt   => { lexer.next_token(); let r = eval_additive_dom(lexer, scope, dom); val = JsValue::Bool(val.to_number() <  r.to_number()); }
            Token::Gt   => { lexer.next_token(); let r = eval_additive_dom(lexer, scope, dom); val = JsValue::Bool(val.to_number() >  r.to_number()); }
            Token::LtEq => { lexer.next_token(); let r = eval_additive_dom(lexer, scope, dom); val = JsValue::Bool(val.to_number() <= r.to_number()); }
            Token::GtEq => { lexer.next_token(); let r = eval_additive_dom(lexer, scope, dom); val = JsValue::Bool(val.to_number() >= r.to_number()); }
            _ => break,
        }
    }
    val
}

fn eval_additive_dom(lexer: &mut Lexer, scope: &mut Scope, dom: Option<&JsDom<'_>>) -> JsValue {
    let mut val = eval_multiplicative_dom(lexer, scope, dom);
    loop {
        match lexer.peek_token() {
            Token::Plus => {
                lexer.next_token();
                let rhs = eval_multiplicative_dom(lexer, scope, dom);
                val = match (&val, &rhs) {
                    (JsValue::Number(a), JsValue::Number(b)) => JsValue::Number(a + b),
                    _ => JsValue::Str(format!("{}{}", val.to_display(), rhs.to_display())),
                };
            }
            Token::Minus => {
                lexer.next_token();
                let rhs = eval_multiplicative_dom(lexer, scope, dom);
                val = JsValue::Number(val.to_number() - rhs.to_number());
            }
            _ => break,
        }
    }
    val
}

fn eval_multiplicative_dom(lexer: &mut Lexer, scope: &mut Scope, dom: Option<&JsDom<'_>>) -> JsValue {
    let mut val = eval_unary_dom(lexer, scope, dom);
    loop {
        match lexer.peek_token() {
            Token::Star    => { lexer.next_token(); let r = eval_unary_dom(lexer, scope, dom); val = JsValue::Number(val.to_number() * r.to_number()); }
            Token::Slash   => { lexer.next_token(); let r = eval_unary_dom(lexer, scope, dom); val = JsValue::Number(val.to_number() / r.to_number()); }
            Token::Percent => { lexer.next_token(); let r = eval_unary_dom(lexer, scope, dom); val = JsValue::Number(val.to_number() % r.to_number()); }
            _ => break,
        }
    }
    val
}

fn eval_unary_dom(lexer: &mut Lexer, scope: &mut Scope, dom: Option<&JsDom<'_>>) -> JsValue {
    match lexer.peek_token() {
        Token::Bang  => { lexer.next_token(); let v = eval_unary_dom(lexer, scope, dom); JsValue::Bool(!v.to_bool()) }
        Token::Minus => { lexer.next_token(); let v = eval_unary_dom(lexer, scope, dom); JsValue::Number(-v.to_number()) }
        _ => eval_primary_dom(lexer, scope, dom),
    }
}

fn eval_primary_dom(lexer: &mut Lexer, scope: &mut Scope, dom: Option<&JsDom<'_>>) -> JsValue {
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
            "document"  => eval_document_expr(lexer, scope, dom),
            name => {
                let val = scope.get(name);
                if lexer.peek_token() == Token::Dot {
                    if let JsValue::Str(ref s) = val {
                        if s.starts_with("\x00elem\x00") || s.starts_with("\x00list\x00") {
                            return chain_props_inner(lexer, val);
                        }
                    }
                    // Unknown object — skip the dot-chain
                    crate::js::eval::skip_dot_call_chain(lexer);
                    return JsValue::Undefined;
                }
                // Function call on a non-object variable — skip args
                if lexer.peek_token() == Token::LParen {
                    skip_call_args(lexer);
                    return JsValue::Undefined;
                }
                val
            }
        },
        Token::LParen => {
            let inner = eval_expr_with_dom(lexer, scope, dom);
            if lexer.peek_token() == Token::RParen { lexer.next_token(); }
            inner
        }
        // Array literal or bracket access — skip and return Undefined
        Token::LBracket => {
            skip_balanced(lexer, Token::LBracket, Token::RBracket);
            JsValue::Undefined
        }
        // Object literal — skip and return Undefined
        Token::LBrace => {
            skip_block(lexer);
            JsValue::Undefined
        }
        _ => JsValue::Undefined,
    }
}

// ---------------------------------------------------------------------------
// Skip helpers
// ---------------------------------------------------------------------------

/// Skip a complete statement, handling blocks, parens, brackets correctly.
/// Consumes until (and including) `;`, or until a `}` closes a block.
pub fn skip_statement(lexer: &mut Lexer) {
    match lexer.peek_token() {
        // A block — consume the whole thing
        Token::LBrace => { skip_block(lexer); }
        Token::Eof    => {}
        _ => {
            // Consume tokens, respecting nesting, until `;` or a `}` at depth 0
            let mut paren_depth  = 0i32;
            let mut bracket_depth = 0i32;
            loop {
                match lexer.peek_token() {
                    Token::Eof => break,
                    Token::Semi => { lexer.next_token(); break; }
                    // A `{` at depth 0 in a paren means we've hit a block body
                    // (e.g. `if (x) {`); skip the whole block then stop.
                    Token::LBrace if paren_depth == 0 && bracket_depth == 0 => {
                        skip_block(lexer);
                        // Optionally skip an `else { ... }` that follows
                        if let Token::Ident(ref id) = lexer.peek_token() {
                            if id == "else" {
                                lexer.next_token();
                                skip_statement(lexer);
                            }
                        }
                        break;
                    }
                    Token::RBrace if paren_depth == 0 && bracket_depth == 0 => {
                        // Don't consume — let the outer loop eat it
                        break;
                    }
                    Token::LParen   => { lexer.next_token(); paren_depth += 1; }
                    Token::RParen   => {
                        lexer.next_token();
                        paren_depth = (paren_depth - 1).max(0);
                        // Arrow function: `) =>` — skip the body
                        if lexer.peek_token() == Token::Eq {
                            let saved = lexer.pos;
                            lexer.next_token(); // consume `=`
                            if lexer.peek_token() == Token::Gt {
                                lexer.next_token(); // consume `>`
                                skip_statement(lexer);
                                break;
                            } else {
                                lexer.pos = saved;
                            }
                        }
                    }
                    Token::LBracket  => { lexer.next_token(); bracket_depth += 1; }
                    Token::RBracket  => { lexer.next_token(); bracket_depth = (bracket_depth - 1).max(0); }
                    _ => { lexer.next_token(); }
                }
            }
        }
    }
}

/// Skip a `{ ... }` block, respecting nested braces.
fn skip_block(lexer: &mut Lexer) {
    if lexer.peek_token() != Token::LBrace { return; }
    lexer.next_token(); // consume `{`
    let mut depth = 1i32;
    loop {
        match lexer.next_token() {
            Token::Eof    => break,
            Token::LBrace => depth += 1,
            Token::RBrace => {
                depth -= 1;
                if depth == 0 { break; }
            }
            _ => {}
        }
    }
}

/// Skip a balanced pair (LBracket..RBracket or LParen..RParen).
fn skip_balanced(lexer: &mut Lexer, _open: Token, close: Token) {
    lexer.next_token(); // consume opening token
    let mut depth = 1i32;
    loop {
        let tok = lexer.next_token();
        if tok == Token::Eof { break; }
        if tok == Token::LBracket || tok == Token::LParen || tok == Token::LBrace {
            depth += 1;
        } else if tok == close || tok == Token::RBracket || tok == Token::RParen || tok == Token::RBrace {
            depth -= 1;
            if depth == 0 { break; }
        }
    }
}

/// Skip a function call argument list `(...)`.
fn skip_call_args(lexer: &mut Lexer) {
    if lexer.peek_token() != Token::LParen { return; }
    skip_balanced(lexer, Token::LParen, Token::RParen);
}

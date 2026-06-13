/// Shared expression helpers used by the DOM-aware interpreter.
///
/// The full precedence tower lives in interpreter.rs (eval_*_dom functions)
/// because it needs to thread the DOM context through every call.  This module
/// keeps the pieces that are context-free and reused from multiple places:
///
///   - js_loose_eq    — JS `==` semantics
///   - skip_*         — consume tokens without evaluating (short-circuit paths)

use crate::js::lexer::{Lexer, Token};

// ---------------------------------------------------------------------------
// Loose equality (==) — simplified JS semantics
// ---------------------------------------------------------------------------
use crate::js::types::JsValue;

pub fn js_loose_eq(a: &JsValue, b: &JsValue) -> bool {
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

// ---------------------------------------------------------------------------
// Skip helpers — consume tokens for short-circuit paths without evaluating
// ---------------------------------------------------------------------------

pub fn skip_and_expr(lexer: &mut Lexer) {
    skip_expr_at_precedence(lexer, Prec::And);
}

pub fn skip_equality_expr(lexer: &mut Lexer) {
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

/// Skip `foo.bar(...)` dot-call chains after the leading ident is already consumed.
pub fn skip_dot_call_chain(lexer: &mut Lexer) {
    loop {
        if lexer.peek_token() != Token::Dot { break; }
        lexer.next_token(); // consume .
        if let Token::Ident(_) = lexer.peek_token() { lexer.next_token(); }
        // optional argument list
        if lexer.peek_token() == Token::LParen {
            lexer.next_token(); // consume (
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

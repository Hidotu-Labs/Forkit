use std::sync::{Arc, Mutex};
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
/// Execute JS without any DOM access (original behaviour).
pub fn execute(src: &str) -> Vec<ConsoleEntry> {
    let mut scope = Scope::new();
    execute_inner_with_scope(src, None, &mut scope)
}

/// Execute JS with read access to the parsed DOM.
pub fn execute_with_dom<'a>(src: &str, dom: &'a JsDom<'a>) -> Vec<ConsoleEntry> {
    let mut scope = Scope::new();
    execute_inner_with_scope(src, Some(dom), &mut scope)
}

/// Execute JS with read access to the parsed DOM and a persistent scope.
pub fn execute_with_dom_and_scope<'a>(src: &str, dom: &'a JsDom<'a>, scope: &mut Scope) -> Vec<ConsoleEntry> {
    execute_inner_with_scope(src, Some(dom), scope)
}

/// Execute a specific JS function with arguments.
pub fn execute_function(
    func: &crate::js::types::JsFunction,
    args: Vec<JsValue>,
    dom:  &JsDom<'_>,
    scope: &mut Scope,
) -> Vec<ConsoleEntry> {
    let mut entries = Vec::new();
    // call_function is internal but we want to expose it via this wrapper.
    let _ = call_function(func, args, &mut entries, Some(dom), scope, 0);
    entries
}

/// Pre-scan the source for top-level `function name(params){body}` declarations
/// and register them in scope so that calls before the declaration work (hoisting).
fn hoist_functions(src: &str, scope: &mut Scope) {
    let mut lexer = Lexer::new(src);
    loop {
        match lexer.peek_token() {
            Token::Eof => break,
            Token::Ident(ref id) if id == "function" => {
                // consume `function`
                lexer.next_token();
                // must be followed by a name
                let name = match lexer.next_token() {
                    Token::Ident(n) => n,
                    _ => continue,
                };
                if name.is_empty() { continue; }
                let func = parse_function_value(&mut lexer);
                scope.declare(&name, JsValue::Function(Box::new(func)));
            }
            _ => { lexer.next_token(); }
        }
    }
}

fn execute_inner_with_scope(src: &str, dom: Option<&JsDom<'_>>, scope: &mut Scope) -> Vec<ConsoleEntry> {
    let mut entries = Vec::new();
    let mut lexer   = Lexer::new(src);
    // Hoist all top-level function declarations so they are available before
    // the line they appear on (standard JS hoisting behaviour).
    hoist_functions(src, scope);
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
        let _ = run_statement(&mut lexer, scope, &mut entries, dom);

        // Safety: if run_statement consumed nothing, forcibly advance one token
        // to prevent an infinite loop on any token we don't handle.
        if lexer.pos == pos_before {
            lexer.next_token();
        }
    }
    // Drain any console entries produced inside function calls
    entries.extend(scope.entries.drain(..));
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
) -> Signal {
    match lexer.peek_token() {
        Token::Ident(ref id) if id == "console" => {
            handle_console(lexer, scope, entries, dom);
            return Signal::None;
        }
        Token::Ident(ref id) if id == "document" => {
            handle_document_stmt(lexer, scope, entries, dom);
            return Signal::None;
        }
        Token::Ident(ref id) if id == "setTimeout" => {
            handle_set_timeout(lexer, scope, entries, dom);
            return Signal::None;
        }
        // ── Variable declaration: let / const / var ──────────────────────────
        Token::Ident(ref id) if matches!(id.as_str(), "var" | "let" | "const") => {
            lexer.next_token(); // consume keyword
            let name = match lexer.next_token() {
                Token::Ident(n) => n,
                _ => { skip_statement(lexer); return Signal::None; }
            };
            // Handle destructuring: let { a, b } = ... or let [ a ] = ...
            if matches!(name.as_str(), "") {
                skip_statement(lexer);
                return Signal::None;
            }
            let val = if lexer.peek_token() == Token::Eq {
                lexer.next_token();
                eval_expr_with_dom(lexer, scope, dom)
            } else {
                JsValue::Undefined
            };
            scope.declare(&name, val);
            if lexer.peek_token() == Token::Semi { lexer.next_token(); }
            return Signal::None;
        }

        // ── Prefix ++ / -- ───────────────────────────────────────────────────
        Token::PlusPlus | Token::MinusMinus => {
            let op = lexer.next_token();
            if let Token::Ident(name) = lexer.next_token() {
                let cur = scope.get(&name).to_number();
                let new = if op == Token::PlusPlus { cur + 1.0 } else { cur - 1.0 };
                scope.set(&name, JsValue::Number(new));
            }
            if lexer.peek_token() == Token::Semi { lexer.next_token(); }
            return Signal::None;
        }

        // ── if / else ────────────────────────────────────────────────────────
        Token::Ident(ref id) if id == "if" => {
            return run_if(lexer, scope, entries, dom);
        }

        // ── while ────────────────────────────────────────────────────────────
        Token::Ident(ref id) if id == "while" => {
            let sig = run_while(lexer, scope, entries, dom);
            return sig;
        }

        // ── do...while ───────────────────────────────────────────────────────
        Token::Ident(ref id) if id == "do" => {
            let sig = run_do_while(lexer, scope, entries, dom);
            return sig;
        }

        // ── for ──────────────────────────────────────────────────────────────
        Token::Ident(ref id) if id == "for" => {
            let sig = run_for(lexer, scope, entries, dom);
            return sig;
        }

        // ── function declaration ──────────────────────────────────────────────
        Token::Ident(ref id) if id == "function" => {
            run_function_decl(lexer, scope);
            return Signal::None;
        }

        // ── return ───────────────────────────────────────────────────────────
        Token::Ident(ref id) if id == "return" => {
            lexer.next_token(); // consume `return`
            let val = match lexer.peek_token() {
                Token::Semi | Token::RBrace | Token::Eof => JsValue::Undefined,
                _ => eval_expr_with_dom(lexer, scope, dom),
            };
            if lexer.peek_token() == Token::Semi { lexer.next_token(); }
            return Signal::Return(val);
        }

        // ── break / continue ─────────────────────────────────────────────────
        Token::Ident(ref id) if id == "break" => {
            lexer.next_token();
            if lexer.peek_token() == Token::Semi { lexer.next_token(); }
            return Signal::Break;
        }
        Token::Ident(ref id) if id == "continue" => {
            lexer.next_token();
            if lexer.peek_token() == Token::Semi { lexer.next_token(); }
            return Signal::Continue;
        }

        // ── Keywords that introduce blocks we can't execute — skip them ───────
        Token::Ident(ref id) if matches!(id.as_str(),
            "else" | "switch" |
            "class" | "throw" | "try" |
            "catch" | "finally" | "import" | "export" | "async" |
            "await" | "yield" | "delete" | "typeof" | "void" |
            "new" | "debugger" | "with"
        ) => {
            skip_statement(lexer);
            return Signal::None;
        }

        // ── Bare block: { ... } ───────────────────────────────────────────────
        Token::LBrace => {
            return run_block_signal(lexer, scope, entries, dom);
        }

        // ── Identifier-starting statement ────────────────────────────────────
        Token::Ident(_) => {
            let saved = lexer.pos;
            let name = match lexer.next_token() {
                Token::Ident(n) => n,
                _ => { lexer.pos = saved; skip_statement(lexer); return Signal::None; }
            };

            match lexer.peek_token() {
                // Simple assignment: name = expr
                Token::Eq => {
                    lexer.next_token();
                    let val = eval_expr_with_dom(lexer, scope, dom);
                    scope.set(&name, val);
                    if lexer.peek_token() == Token::Semi { lexer.next_token(); }
                    return Signal::None;
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
                    return Signal::None;
                }
                // Postfix ++ / --
                Token::PlusPlus => {
                    lexer.next_token();
                    let cur = scope.get(&name).to_number();
                    scope.set(&name, JsValue::Number(cur + 1.0));
                    if lexer.peek_token() == Token::Semi { lexer.next_token(); }
                    return Signal::None;
                }
                Token::MinusMinus => {
                    lexer.next_token();
                    let cur = scope.get(&name).to_number();
                    scope.set(&name, JsValue::Number(cur - 1.0));
                    if lexer.peek_token() == Token::Semi { lexer.next_token(); }
                    return Signal::None;
                }
                // Dot access — may be a write (el.prop = val) or array method call, or DOM element method
                Token::Dot => {
                    let var_val = scope.get(&name);
                    // ── Array method call as statement: arr.push(...), arr.pop(), etc. ──
                    if let JsValue::Array(ref arr) = var_val {
                        let arr = arr.clone();
                        lexer.next_token(); // consume `.`
                        let method = match lexer.next_token() {
                            Token::Ident(m) => m,
                            _ => { lexer.pos = saved; skip_statement(lexer); return Signal::None; }
                        };
                        eval_array_method_full(lexer, scope, dom, &arr, &method);
                        if lexer.peek_token() == Token::Semi { lexer.next_token(); }
                        return Signal::None;
                    }
                    // ── DOM element write or method call ─────────────────────────────────
                    if let JsValue::Str(ref s) = var_val {
                        if s.starts_with("\x00elem\x00") {
                            let encoded = s.clone();
                            if handle_element_write(lexer, scope, entries, dom, &encoded, &name) {
                                if lexer.peek_token() == Token::Semi { lexer.next_token(); }
                                return Signal::None;
                            }
                        }
                    }
                    // restore and fall through to console/document handler
                    lexer.pos = saved;
                }
                // Array index write: name[index] = value
                Token::LBracket => {
                    let var_val = scope.get(&name);
                    if let JsValue::Array(ref arr) = var_val {
                        let arr = arr.clone();
                        lexer.next_token(); // consume `[`
                        let idx_val = eval_expr_with_dom(lexer, scope, dom);
                        if lexer.peek_token() == Token::RBracket { lexer.next_token(); }
                        if lexer.peek_token() == Token::Eq {
                            lexer.next_token();
                            let rhs = eval_expr_with_dom(lexer, scope, dom);
                            let i = idx_val.to_number();
                            if i >= 0.0 && i.fract() == 0.0 {
                                let i = i as usize;
                                let mut v = arr.lock().unwrap();
                                if i >= v.len() { v.resize(i + 1, JsValue::Undefined); }
                                v[i] = rhs;
                            }
                        }
                        if lexer.peek_token() == Token::Semi { lexer.next_token(); }
                        return Signal::None;
                    }
                    // Not an array — fall through / skip
                    lexer.pos = saved;
                    skip_statement(lexer);
                    return Signal::None;
                }
                // Array method call as statement: arr.push(...) etc.
                _ => {
                    // Check if it's an array method call: arr.push(v) etc.
                    let var_val = scope.get(&name);
                    if let JsValue::Array(ref arr) = var_val {
                        if lexer.peek_token() == Token::Dot {
                            let arr = arr.clone();
                            lexer.next_token(); // consume `.`
                            let method = match lexer.next_token() {
                                Token::Ident(m) => m,
                                _ => { lexer.pos = saved; skip_statement(lexer); return Signal::None; }
                            };
                            eval_array_method_full(lexer, scope, dom, &arr, &method);
                            if lexer.peek_token() == Token::Semi { lexer.next_token(); }
                            return Signal::None;
                        }
                    }
                    // Check if it's a bare function call: `foo(...)` or `foo.bar(...)`
                    if lexer.peek_token() == Token::LParen {
                        let val = scope.get(&name);
                        if let JsValue::Function(ref func) = val {
                            let func = func.as_ref().clone();
                            let args = eval_arg_list(lexer, scope, dom);
                            let mut dummy = Vec::new();
                            call_function(&func, args, &mut dummy, dom, scope, 0);
                            if lexer.peek_token() == Token::Semi { lexer.next_token(); }
                            return Signal::None;
                        }
                    }
                    lexer.pos = saved;
                    skip_statement(lexer);
                    return Signal::None;
                }
            }
        }

        _ => {
            // Unknown token at statement level — skip it
            skip_statement(lexer);
            return Signal::None;
        }
    }

    // ── Dispatch on the leading identifier (console / document) ──────────────
    let leading = match lexer.peek_token() {
        Token::Ident(s) => s,
        _ => { skip_statement(lexer); return Signal::None; }
    };

    match leading.as_str() {
        "console"  => handle_console(lexer, scope, entries, dom),
        "document" => { handle_document_stmt(lexer, scope, entries, dom); }
        _          => { skip_statement(lexer); }
    }
    Signal::None
}

// ---------------------------------------------------------------------------
// Control flow — if / else
// ---------------------------------------------------------------------------

/// `if (cond) { body } else if (...) { } else { }`
fn run_if(
    lexer:   &mut Lexer,
    scope:   &mut Scope,
    entries: &mut Vec<ConsoleEntry>,
    dom:     Option<&JsDom<'_>>,
) -> Signal {
    lexer.next_token(); // consume `if`

    // Parse condition
    if lexer.peek_token() != Token::LParen { skip_statement(lexer); return Signal::None; }
    lexer.next_token(); // consume `(`
    let cond = eval_expr_with_dom(lexer, scope, dom).to_bool();
    if lexer.peek_token() == Token::RParen { lexer.next_token(); }

    // Execute or skip the then-branch
    let sig = if cond {
        run_block_or_stmt_signal(lexer, scope, entries, dom)
    } else {
        skip_block_or_stmt(lexer);
        Signal::None
    };

    // Propagate break/continue/return immediately — don't consume else branches
    if !matches!(sig, Signal::None) {
        // Still need to skip any trailing else chain
        loop {
            while lexer.peek_token() == Token::Semi { lexer.next_token(); }
            if let Token::Ident(ref id) = lexer.peek_token() {
                if id == "else" {
                    lexer.next_token();
                    skip_block_or_stmt(lexer);
                    continue;
                }
            }
            break;
        }
        return sig;
    }

    // Handle else / else if
    loop {
        while lexer.peek_token() == Token::Semi { lexer.next_token(); }
        match lexer.peek_token() {
            Token::Ident(ref id) if id == "else" => {
                lexer.next_token(); // consume `else`
                match lexer.peek_token() {
                    Token::Ident(ref id) if id == "if" => {
                        if cond {
                            // Already took a branch — skip this else-if chain
                            skip_if_chain(lexer);
                            break;
                        } else {
                            // Try this else-if; it handles its own else chain
                            let s = run_if(lexer, scope, entries, dom);
                            return s;
                        }
                    }
                    _ => {
                        // Plain else
                        let s = if cond {
                            skip_block_or_stmt(lexer);
                            Signal::None
                        } else {
                            run_block_or_stmt_signal(lexer, scope, entries, dom)
                        };
                        return s;
                    }
                }
            }
            _ => break,
        }
    }
    Signal::None
}

/// Skip a full `if (...) {...} else if (...) {...} else {...}` chain
/// without executing anything.
fn skip_if_chain(lexer: &mut Lexer) {
    // skip `if`
    if let Token::Ident(ref id) = lexer.peek_token() {
        if id == "if" { lexer.next_token(); }
    }
    // skip (cond)
    if lexer.peek_token() == Token::LParen {
        skip_balanced(lexer, Token::LParen, Token::RParen);
    }
    // skip body
    skip_block_or_stmt(lexer);
    // skip else chain
    while lexer.peek_token() == Token::Semi { lexer.next_token(); }
    if let Token::Ident(ref id) = lexer.peek_token() {
        if id == "else" {
            lexer.next_token();
            skip_if_chain(lexer);
        }
    }
}

// ---------------------------------------------------------------------------
// Control flow — while
// ---------------------------------------------------------------------------

fn run_while(
    lexer:   &mut Lexer,
    scope:   &mut Scope,
    entries: &mut Vec<ConsoleEntry>,
    dom:     Option<&JsDom<'_>>,
) -> Signal {
    lexer.next_token(); // consume `while`

    if lexer.peek_token() != Token::LParen { skip_statement(lexer); return Signal::None; }

    let cond_start = lexer.pos;
    const MAX_ITER: usize = 100_000;
    let mut iters = 0;

    loop {
        if iters >= MAX_ITER { break; }
        iters += 1;

        lexer.pos = cond_start;
        lexer.next_token(); // consume `(`
        let cond = eval_expr_with_dom(lexer, scope, dom).to_bool();
        if lexer.peek_token() == Token::RParen { lexer.next_token(); }

        if !cond { skip_block_or_stmt(lexer); break; }

        let sig = run_block_or_stmt_signal(lexer, scope, entries, dom);
        match sig {
            Signal::Break           => break,
            Signal::Return(_)       => return sig,
            Signal::Continue | Signal::None => {}
        }
    }
    Signal::None
}

// ---------------------------------------------------------------------------
// Control flow — do...while
// ---------------------------------------------------------------------------

fn run_do_while(
    lexer:   &mut Lexer,
    scope:   &mut Scope,
    entries: &mut Vec<ConsoleEntry>,
    dom:     Option<&JsDom<'_>>,
) -> Signal {
    lexer.next_token(); // consume `do`

    let body_start = lexer.pos;
    const MAX_ITER: usize = 100_000;
    let mut iters = 0;

    loop {
        if iters >= MAX_ITER { break; }
        iters += 1;

        lexer.pos = body_start;
        let sig = run_block_or_stmt_signal(lexer, scope, entries, dom);

        // consume `while`
        while lexer.peek_token() == Token::Semi { lexer.next_token(); }
        if let Token::Ident(ref id) = lexer.peek_token() {
            if id == "while" { lexer.next_token(); }
        }

        if lexer.peek_token() != Token::LParen { break; }
        lexer.next_token(); // `(`
        let cond = eval_expr_with_dom(lexer, scope, dom).to_bool();
        if lexer.peek_token() == Token::RParen { lexer.next_token(); }
        if lexer.peek_token() == Token::Semi   { lexer.next_token(); }

        match sig {
            Signal::Break           => break,
            Signal::Return(_)       => return sig,
            Signal::Continue | Signal::None => {}
        }
        if !cond { break; }
    }
    Signal::None
}

// ---------------------------------------------------------------------------
// Control flow — for
// ---------------------------------------------------------------------------

fn run_for(
    lexer:   &mut Lexer,
    scope:   &mut Scope,
    entries: &mut Vec<ConsoleEntry>,
    dom:     Option<&JsDom<'_>>,
) -> Signal {
    lexer.next_token(); // consume `for`

    if lexer.peek_token() != Token::LParen { skip_statement(lexer); return Signal::None; }
    lexer.next_token(); // consume `(`

    // Detect for...of / for...in
    {
        let saved = lexer.pos;
        let kw = match lexer.next_token() {
            Token::Ident(s) => s,
            _ => { lexer.pos = saved; String::new() },
        };
        if matches!(kw.as_str(), "var" | "let" | "const") {
            let var_name = match lexer.next_token() {
                Token::Ident(n) => n,
                _ => { lexer.pos = saved; String::new() },
            };
            if let Token::Ident(ref id) = lexer.peek_token() {
                if id == "of" {
                    lexer.next_token(); // consume `of`
                    let iterable = eval_expr_with_dom(lexer, scope, dom);
                    if lexer.peek_token() == Token::RParen { lexer.next_token(); }

                    let items: Vec<JsValue> = match iterable {
                        JsValue::Array(arr) => arr.lock().unwrap().clone(),
                        _ => Vec::new(),
                    };

                    scope.push();
                    let mut ret = Signal::None;
                    let body_start = lexer.pos;
                    for item in items {
                        lexer.pos = body_start;
                        scope.declare(&var_name, item);
                        let sig = run_block_or_stmt_signal(lexer, scope, entries, dom);
                        match sig {
                            Signal::Break           => break,
                            Signal::Return(_)       => { ret = sig; break; }
                            Signal::Continue | Signal::None => {}
                        }
                    }
                    scope.pop();
                    return ret;
                } else if id == "in" {
                    // for...in — skip
                    let mut depth = 1i32;
                    loop {
                        match lexer.next_token() {
                            Token::Eof => break,
                            Token::LParen => depth += 1,
                            Token::RParen => { depth -= 1; if depth == 0 { break; } }
                            _ => {}
                        }
                    }
                    skip_block_or_stmt(lexer);
                    return Signal::None;
                }
            }
        }
        lexer.pos = saved;
    }

    // Standard for (init; cond; update)
    scope.push();

    run_for_init(lexer, scope, entries, dom);
    if lexer.peek_token() == Token::Semi { lexer.next_token(); }

    let cond_start = lexer.pos;
    const MAX_ITER: usize = 100_000;
    let mut iters = 0;

    let update_start;
    let body_start;
    {
        let saved = lexer.pos;
        skip_for_clause(lexer);
        if lexer.peek_token() == Token::Semi { lexer.next_token(); }
        update_start = lexer.pos;
        skip_for_clause(lexer);
        if lexer.peek_token() == Token::RParen { lexer.next_token(); }
        body_start = lexer.pos;
        lexer.pos = saved;
    }

    let mut ret = Signal::None;
    loop {
        if iters >= MAX_ITER { break; }
        iters += 1;

        lexer.pos = cond_start;
        let cond = if lexer.peek_token() == Token::Semi {
            true
        } else {
            eval_expr_with_dom(lexer, scope, dom).to_bool()
        };

        if !cond {
            lexer.pos = body_start;
            skip_block_or_stmt(lexer);
            break;
        }

        lexer.pos = body_start;
        let sig = run_block_or_stmt_signal(lexer, scope, entries, dom);
        match sig {
            Signal::Break           => break,
            Signal::Return(_)       => { ret = sig; break; }
            Signal::Continue | Signal::None => {}
        }

        lexer.pos = update_start;
        if lexer.peek_token() != Token::RParen && lexer.peek_token() != Token::Eof {
            run_for_update(lexer, scope, dom);
        }
    }

    scope.pop();
    ret
}

/// Execute a for-loop init clause (before first `;`).
fn run_for_init(
    lexer:   &mut Lexer,
    scope:   &mut Scope,
    _entries: &mut Vec<ConsoleEntry>,
    dom:     Option<&JsDom<'_>>,
) {
    match lexer.peek_token() {
        Token::Semi | Token::Eof => {} // empty init
        Token::Ident(ref id) if matches!(id.as_str(), "var" | "let" | "const") => {
            lexer.next_token();
            // May be comma-separated: `let i = 0, j = 0`
            loop {
                let name = match lexer.next_token() {
                    Token::Ident(n) => n,
                    _ => break,
                };
                let val = if lexer.peek_token() == Token::Eq {
                    lexer.next_token();
                    eval_expr_with_dom(lexer, scope, dom)
                } else {
                    JsValue::Undefined
                };
                scope.declare(&name, val);
                if lexer.peek_token() == Token::Comma {
                    lexer.next_token();
                } else {
                    break;
                }
            }
        }
        _ => {
            // Expression init (e.g. `i = 0`)
            eval_expr_with_dom(lexer, scope, dom);
        }
    }
}

/// Execute a for-loop update clause (after second `;`, before `)`).
fn run_for_update(
    lexer: &mut Lexer,
    scope: &mut Scope,
    dom:   Option<&JsDom<'_>>,
) {
    // Parse comma-separated update expressions
    loop {
        match lexer.peek_token() {
            Token::RParen | Token::Eof | Token::Semi => break,
            Token::Ident(_) => {
                let saved = lexer.pos;
                let name = match lexer.next_token() {
                    Token::Ident(n) => n,
                    _ => { lexer.pos = saved; break; }
                };
                match lexer.peek_token() {
                    Token::PlusPlus => {
                        lexer.next_token();
                        let v = scope.get(&name).to_number();
                        scope.set(&name, JsValue::Number(v + 1.0));
                    }
                    Token::MinusMinus => {
                        lexer.next_token();
                        let v = scope.get(&name).to_number();
                        scope.set(&name, JsValue::Number(v - 1.0));
                    }
                    Token::Eq => {
                        lexer.next_token();
                        let val = eval_expr_with_dom(lexer, scope, dom);
                        scope.set(&name, val);
                    }
                    Token::PlusEq | Token::MinusEq | Token::StarEq | Token::SlashEq => {
                        let op = lexer.next_token();
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
                    }
                    _ => {
                        lexer.pos = saved;
                        eval_expr_with_dom(lexer, scope, dom);
                    }
                }
            }
            _ => { eval_expr_with_dom(lexer, scope, dom); }
        }
        if lexer.peek_token() == Token::Comma { lexer.next_token(); } else { break; }
    }
}

/// Skip tokens for one for-header clause (stops at `;` or `)` at depth 0).
fn skip_for_clause(lexer: &mut Lexer) {
    let mut depth = 0i32;
    loop {
        match lexer.peek_token() {
            Token::Eof => break,
            Token::Semi if depth == 0 => break,
            Token::RParen if depth == 0 => break,
            Token::LParen => { lexer.next_token(); depth += 1; }
            Token::RParen => { lexer.next_token(); depth -= 1; }
            _ => { lexer.next_token(); }
        }
    }
}

// ---------------------------------------------------------------------------
// Signal — break / continue / return propagation
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum Signal {
    None,
    Break,
    Continue,
    Return(JsValue),
}

// ---------------------------------------------------------------------------
// Block / statement runners
// ---------------------------------------------------------------------------

/// Run either a `{ ... }` block or a single statement, with a scope frame.
fn run_block_or_stmt(
    lexer:   &mut Lexer,
    scope:   &mut Scope,
    entries: &mut Vec<ConsoleEntry>,
    dom:     Option<&JsDom<'_>>,
) {
    let _ = run_block_or_stmt_signal(lexer, scope, entries, dom);
}

/// Same as `run_block_or_stmt` but propagates break/continue signals.
fn run_block_or_stmt_signal(
    lexer:   &mut Lexer,
    scope:   &mut Scope,
    entries: &mut Vec<ConsoleEntry>,
    dom:     Option<&JsDom<'_>>,
) -> Signal {
    if lexer.peek_token() == Token::LBrace {
        run_block_signal(lexer, scope, entries, dom)
    } else {
        run_statement(lexer, scope, entries, dom)
    }
}

/// Run a `{ ... }` block, pushing/popping a scope frame.
/// Returns the first non-None signal seen.
fn run_block_signal(
    lexer:   &mut Lexer,
    scope:   &mut Scope,
    entries: &mut Vec<ConsoleEntry>,
    dom:     Option<&JsDom<'_>>,
) -> Signal {
    if lexer.peek_token() != Token::LBrace { return Signal::None; }
    lexer.next_token(); // consume `{`

    scope.push();
    let mut signal = Signal::None;

    loop {
        // Eat bare semicolons
        while lexer.peek_token() == Token::Semi { lexer.next_token(); }

        match lexer.peek_token() {
            Token::RBrace | Token::Eof => break,
            _ => {}
        }

        let pos_before = lexer.pos;
        let sig = run_statement(lexer, scope, entries, dom);
        if lexer.pos == pos_before { lexer.next_token(); } // safety advance
        if !matches!(sig, Signal::None) {
            signal = sig;
            break;
        }
    }

    if lexer.peek_token() == Token::RBrace { lexer.next_token(); }
    scope.pop();
    signal
}

/// Skip a `{ ... }` block or a single statement without executing.
fn skip_block_or_stmt(lexer: &mut Lexer) {
    if lexer.peek_token() == Token::LBrace {
        skip_block(lexer);
    } else {
        skip_statement(lexer);
    }
}

// ---------------------------------------------------------------------------
// Function declaration — `function name(params) { body }`
// Also stores anonymous: `function(params) { body }` returned as a value.
// ---------------------------------------------------------------------------

/// Parse and store a `function name(params) { body }` declaration.
/// The function value is stored in `scope` under `name`.
fn run_function_decl(lexer: &mut Lexer, scope: &mut Scope) {
    lexer.next_token(); // consume `function`

    // Optional name (anonymous: `function() {}`)
    let name = if let Token::Ident(_) = lexer.peek_token() {
        match lexer.next_token() { Token::Ident(n) => n, _ => String::new() }
    } else {
        String::new()
    };

    let func = parse_function_value(lexer);
    if !name.is_empty() {
        scope.declare(&name, JsValue::Function(Box::new(func)));
    }
}

/// Parse `(params) { body }` or `(params) => expr/block` starting at `(`.
/// Returns a `JsFunction` ready to be called.
fn parse_function_value(lexer: &mut Lexer) -> crate::js::types::JsFunction {
    use crate::js::types::JsFunction;

    // Parse parameter list
    let params = parse_param_list(lexer);

    // Check for arrow `=>`
    let is_arrow = if lexer.peek_token() == Token::Eq {
        let saved = lexer.pos;
        lexer.next_token(); // consume `=`
        if lexer.peek_token() == Token::Gt {
            lexer.next_token(); // consume `>`
            true
        } else {
            lexer.pos = saved;
            false
        }
    } else { false };

    // Capture body source text
    if is_arrow {
        if lexer.peek_token() == Token::LBrace {
            let body = capture_block_src(lexer);
            JsFunction { params, body, is_expr_body: false }
        } else {
            // Expression body — capture until `;` or `,` or `)` at depth 0
            let body = capture_expr_src(lexer);
            JsFunction { params, body, is_expr_body: true }
        }
    } else {
        let body = capture_block_src(lexer);
        JsFunction { params, body, is_expr_body: false }
    }
}

/// Parse `(a, b, c)` returning vec of param names.  Lexer is at `(`.
fn parse_param_list(lexer: &mut Lexer) -> Vec<String> {
    let mut params = Vec::new();
    if lexer.peek_token() != Token::LParen { return params; }
    lexer.next_token(); // consume `(`
    loop {
        match lexer.peek_token() {
            Token::RParen | Token::Eof => break,
            Token::Ident(_) => {
                if let Token::Ident(p) = lexer.next_token() { params.push(p); }
                if lexer.peek_token() == Token::Comma { lexer.next_token(); }
            }
            _ => { lexer.next_token(); } // skip `...rest`, destructuring etc.
        }
    }
    if lexer.peek_token() == Token::RParen { lexer.next_token(); }
    params
}

/// Capture the source text of a `{ ... }` block (including braces).
fn capture_block_src(lexer: &mut Lexer) -> String {
    if lexer.peek_token() != Token::LBrace { return String::new(); }
    let start = lexer.pos;
    skip_block(lexer);
    let end = lexer.pos;
    lexer.chars[start..end].iter().collect()
}

/// Capture a bare expression body for arrow functions, stopping at
/// `;`, unmatched `)`, `}`, or EOF.
fn capture_expr_src(lexer: &mut Lexer) -> String {
    let start = lexer.pos;
    let mut depth = 0i32;
    loop {
        match lexer.peek_token() {
            Token::Eof  => break,
            Token::Semi => break,
            Token::RParen | Token::RBrace if depth == 0 => break,
            Token::LParen | Token::LBrace | Token::LBracket => { lexer.next_token(); depth += 1; }
            Token::RParen | Token::RBrace | Token::RBracket => { lexer.next_token(); depth -= 1; }
            _ => { lexer.next_token(); }
        }
    }
    let end = lexer.pos;
    lexer.chars[start..end].iter().collect()
}

// ---------------------------------------------------------------------------
// Function call
// ---------------------------------------------------------------------------

const MAX_CALL_DEPTH: usize = 32;

/// Call a stored `JsFunction` with evaluated argument values.
/// Console output produced inside the function is appended to `scope.entries`.
fn call_function(
    func:    &crate::js::types::JsFunction,
    args:    Vec<JsValue>,
    _entries: &mut Vec<ConsoleEntry>,
    dom:     Option<&JsDom<'_>>,
    scope:   &mut Scope,
    _depth:  usize,
) -> JsValue {
    if scope.call_depth >= MAX_CALL_DEPTH { return JsValue::Undefined; }
    scope.call_depth += 1;

    let body_src = if func.is_expr_body {
        format!("{{ return {}; }}", func.body)
    } else {
        func.body.clone()
    };

    let mut fn_lexer = Lexer::new(&body_src);
    let mut local_entries: Vec<ConsoleEntry> = Vec::new();

    scope.push();
    for (i, param) in func.params.iter().enumerate() {
        let val = args.get(i).cloned().unwrap_or(JsValue::Undefined);
        scope.declare(param, val);
    }

    let sig = run_block_signal(&mut fn_lexer, scope, &mut local_entries, dom);

    scope.pop();
    scope.call_depth -= 1;

    // Route inner console output to scope.entries so execute_inner can collect it
    scope.entries.append(&mut local_entries);

    match sig {
        Signal::Return(v) => v,
        _                 => JsValue::Undefined,
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
// setTimeout(callback, delay)
// ---------------------------------------------------------------------------

fn handle_set_timeout(
    lexer:   &mut Lexer,
    scope:   &mut Scope,
    _entries: &mut Vec<ConsoleEntry>,
    dom:     Option<&JsDom<'_>>,
) {
    lexer.next_token(); // consume `setTimeout`
    if lexer.next_token() != Token::LParen { return; }

    let callback = eval_expr_with_dom(lexer, scope, dom);
    let mut delay = 0u32;

    if lexer.peek_token() == Token::Comma {
        lexer.next_token(); // consume ,
        delay = eval_expr_with_dom(lexer, scope, dom).to_number() as u32;
    }

    while !matches!(lexer.peek_token(), Token::RParen | Token::Eof) { lexer.next_token(); }
    if lexer.peek_token() == Token::RParen { lexer.next_token(); }
    if lexer.peek_token() == Token::Semi   { lexer.next_token(); }

    if let (JsValue::Function(func), Some(d)) = (callback, dom) {
        d.set_timeout(*func, delay);
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
        return chain_string_props(lexer, val, dom);
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
        "createElement" => {
            let el = dom.map(|d| d.create_element(&arg));
            element_to_value(el)
        }
        _ => JsValue::Undefined,
    }
}

// ---------------------------------------------------------------------------
// Element property chain
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Element write — handles `el.prop = value` and `el.method(...)` mutations.
// Returns true if it consumed the statement (write/method call handled),
// false if the caller should fall through.
// Lexer is positioned just after the variable name, at the `.`.
//
// For detached elements (path is empty, e.g. from createElement), property
// writes update the scope variable's encoded string rather than queuing a DOM
// mutation, so the writes accumulate and are visible when the element is later
// passed to appendChild.
// ---------------------------------------------------------------------------

fn handle_element_write(
    lexer:    &mut Lexer,
    scope:    &mut Scope,
    _entries: &mut Vec<ConsoleEntry>,
    dom:      Option<&JsDom<'_>>,
    encoded:  &str,
    var_name: &str,
) -> bool {
    let mut el = decode_element(encoded);
    let detached = el.path.is_empty();

    if lexer.peek_token() != Token::Dot { return false; }
    lexer.next_token(); // consume `.`

    let prop = match lexer.next_token() {
        Token::Ident(p) => p,
        _ => return false,
    };

    match prop.as_str() {
        // ── el.textContent = "..." ────────────────────────────────────────────
        "textContent" | "innerText" if lexer.peek_token() == Token::Eq => {
            lexer.next_token();
            let val = eval_expr_with_dom(lexer, scope, dom).to_display();
            if detached {
                el.text_content = val.clone();
                el.inner_html   = val;
                scope.set(var_name, JsValue::Str(encode_element(&el)));
            } else if let Some(d) = dom {
                d.push_mutation(crate::js::dom::DomMutation::SetTextContent {
                    path: el.path.clone(), value: val,
                });
            }
            true
        }
        // ── el.innerHTML = "..." ─────────────────────────────────────────────
        "innerHTML" if lexer.peek_token() == Token::Eq => {
            lexer.next_token();
            let val = eval_expr_with_dom(lexer, scope, dom).to_display();
            if detached {
                el.inner_html   = val.clone();
                el.text_content = val; // approximate
                scope.set(var_name, JsValue::Str(encode_element(&el)));
            } else if let Some(d) = dom {
                d.push_mutation(crate::js::dom::DomMutation::SetInnerHtml {
                    path: el.path.clone(), value: val,
                });
            }
            true
        }
        // ── el.className = "..." ─────────────────────────────────────────────
        "className" if lexer.peek_token() == Token::Eq => {
            lexer.next_token();
            let val = eval_expr_with_dom(lexer, scope, dom).to_display();
            if detached {
                el.class_name = val;
                scope.set(var_name, JsValue::Str(encode_element(&el)));
            } else if let Some(d) = dom {
                d.push_mutation(crate::js::dom::DomMutation::SetClassName {
                    path: el.path.clone(), value: val,
                });
            }
            true
        }
        // ── el.id = "..." ────────────────────────────────────────────────────
        "id" if lexer.peek_token() == Token::Eq => {
            lexer.next_token();
            let val = eval_expr_with_dom(lexer, scope, dom).to_display();
            if detached {
                el.id = val;
                scope.set(var_name, JsValue::Str(encode_element(&el)));
            } else if let Some(d) = dom {
                d.push_mutation(crate::js::dom::DomMutation::SetId {
                    path: el.path.clone(), value: val,
                });
            }
            true
        }
        // ── el.setAttribute("name", "value") ────────────────────────────────
        "setAttribute" if lexer.peek_token() == Token::LParen => {
            lexer.next_token();
            let attr_name  = read_arg_str(lexer, scope, dom);
            let attr_value = if lexer.peek_token() == Token::Comma {
                lexer.next_token();
                eval_expr_with_dom(lexer, scope, dom).to_display()
            } else { String::new() };
            while !matches!(lexer.peek_token(), Token::RParen | Token::Eof) { lexer.next_token(); }
            if lexer.peek_token() == Token::RParen { lexer.next_token(); }
            if !detached {
                if let Some(d) = dom {
                    d.push_mutation(crate::js::dom::DomMutation::SetAttribute {
                        path: el.path.clone(), name: attr_name, value: attr_value,
                    });
                }
            }
            true
        }
        // ── el.removeAttribute("name") ───────────────────────────────────────
        "removeAttribute" if lexer.peek_token() == Token::LParen => {
            lexer.next_token();
            let attr_name = read_arg_str(lexer, scope, dom);
            while !matches!(lexer.peek_token(), Token::RParen | Token::Eof) { lexer.next_token(); }
            if lexer.peek_token() == Token::RParen { lexer.next_token(); }
            if !detached {
                if let Some(d) = dom {
                    d.push_mutation(crate::js::dom::DomMutation::RemoveAttribute {
                        path: el.path.clone(), name: attr_name,
                    });
                }
            }
            true
        }
        // ── el.appendChild(child) ────────────────────────────────────────────
        "appendChild" if lexer.peek_token() == Token::LParen => {
            lexer.next_token();
            let child_val = eval_expr_with_dom(lexer, scope, dom);
            while !matches!(lexer.peek_token(), Token::RParen | Token::Eof) { lexer.next_token(); }
            if lexer.peek_token() == Token::RParen { lexer.next_token(); }
            let (child_tag, child_text) = if let JsValue::Str(ref s) = child_val {
                if s.starts_with("\x00elem\x00") {
                    let child_el = decode_element(s);
                    (child_el.tag, child_el.text_content)
                } else {
                    (String::new(), s.clone())
                }
            } else {
                (String::new(), child_val.to_display())
            };
            if !child_tag.is_empty() && !detached {
                if let Some(d) = dom {
                    d.push_mutation(crate::js::dom::DomMutation::AppendChild {
                        path: el.path.clone(), child_tag, child_text,
                    });
                }
            }
            true
        }
        // ── el.remove() ──────────────────────────────────────────────────────
        "remove" if lexer.peek_token() == Token::LParen => {
            lexer.next_token();
            if lexer.peek_token() == Token::RParen { lexer.next_token(); }
            if !detached {
                if let Some(d) = dom {
                    d.push_mutation(crate::js::dom::DomMutation::Remove {
                        path: el.path.clone(),
                    });
                }
            }
            true
        }
        // ── el.addEventListener(type, callback) ──────────────────────────────
        "addEventListener" if lexer.peek_token() == Token::LParen => {
            lexer.next_token();
            let event_type = read_arg_str(lexer, scope, dom);
            let callback = if lexer.peek_token() == Token::Comma {
                lexer.next_token();
                eval_expr_with_dom(lexer, scope, dom)
            } else { JsValue::Null };
            
            while !matches!(lexer.peek_token(), Token::RParen | Token::Eof) { lexer.next_token(); }
            if lexer.peek_token() == Token::RParen { lexer.next_token(); }

            if let (JsValue::Function(func), Some(d)) = (callback, dom) {
                if !detached {
                    d.add_event_listener(el.path.clone(), &event_type, *func);
                }
            }
            true
        }
        _ => false,
    }
}

/// Read the first string argument inside an already-opened `(`.
fn read_arg_str(lexer: &mut Lexer, scope: &mut Scope, dom: Option<&JsDom<'_>>) -> String {
    if matches!(lexer.peek_token(), Token::RParen | Token::Eof) {
        return String::new();
    }
    eval_expr_with_dom(lexer, scope, dom).to_display()
}


fn chain_element_props(
    lexer: &mut Lexer,
    val:   JsValue,
    dom:   Option<&JsDom<'_>>,
) -> JsValue {
    chain_props_inner(lexer, val, dom)
}

/// Chain dot-property/method access on a value.
/// When scope is needed (array callbacks), prefer `chain_with_scope`.
fn chain_props_inner(
    lexer: &mut Lexer,
    val:   JsValue,
    dom:   Option<&JsDom<'_>>,
) -> JsValue {
    if lexer.peek_token() != Token::Dot { return val; }
    lexer.next_token(); // consume `.`

    let prop = match lexer.next_token() {
        Token::Ident(p) => p,
        _ => return val,
    };

    let next_val = if let JsValue::Array(ref arr) = val {
        // No scope — simple non-callback methods only
        eval_array_method(lexer, arr, &prop)
    } else if let JsValue::Str(ref s) = val {
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

    chain_props_inner(lexer, next_val, dom)
}

/// Like `chain_props_inner` but has access to scope — supports full array methods incl. callbacks,
/// and handles chained calls like `.slice(1,3).concat(...)`.
fn chain_with_scope(
    lexer: &mut Lexer,
    val:   JsValue,
    scope: &mut Scope,
    dom:   Option<&JsDom<'_>>,
) -> JsValue {
    if lexer.peek_token() != Token::Dot { return val; }
    lexer.next_token(); // consume `.`

    let prop = match lexer.next_token() {
        Token::Ident(p) => p,
        _ => return val,
    };

    let next_val = if let JsValue::Array(ref arr) = val {
        eval_array_method_full(lexer, scope, dom, arr, &prop)
    } else if let JsValue::Str(ref s) = val {
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

    chain_with_scope(lexer, next_val, scope, dom)
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
        "href"        => JsValue::Str(el.href),
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

fn chain_string_props(lexer: &mut Lexer, val: JsValue, dom: Option<&JsDom<'_>>) -> JsValue {
    chain_props_inner(lexer, val, dom)
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
    let path_str = el.path.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(",");
    format!(
        "\x00elem\x00{}\x00{}\x00{}\x00{}\x00{}\x00{}\x00{}\x00{}\x00{}",
        escape_field(&el.tag),
        escape_field(&el.id),
        escape_field(&el.class_name),
        escape_field(&el.text_content),
        escape_field(&el.inner_html),
        escape_field(&el.attrs_raw),
        el.children.len(),
        escape_field(&path_str),
        escape_field(&el.href),
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
    // Format: \x00elem\x00tag\x00id\x00class\x00text\x00html\x00attrs\x00children_len\x00path
    // Indices:  0      1      2    3    4      5     6     7      8             9
    let path_str = unescape_field(&get(9));
    let path: Vec<usize> = if path_str.is_empty() {
        Vec::new()
    } else {
        path_str.split(',').filter_map(|n| n.parse().ok()).collect()
    };
    JsElement {
        tag:          unescape_field(&get(2)),
        id:           unescape_field(&get(3)),
        class_name:   unescape_field(&get(4)),
        text_content: unescape_field(&get(5)),
        inner_html:   unescape_field(&get(6)),
        attrs_raw:    unescape_field(&get(7)),
        href:         unescape_field(&get(10)),
        children:     Vec::new(),
        path,
    }
}

fn escape_field(s: &str) -> String {
    s.replace('\x00', "\x02").replace('\x01', "\x03")
}

fn unescape_field(s: &str) -> String {
    s.replace('\x02', "\x00").replace('\x03', "\x01")
}

// ---------------------------------------------------------------------------
// Array helpers
// ---------------------------------------------------------------------------

/// Parse an array literal `[expr, expr, ...]`.  The `[` has already been
/// consumed by the caller in `eval_primary_dom`.
fn eval_array_literal(
    lexer: &mut Lexer,
    scope: &mut Scope,
    dom:   Option<&JsDom<'_>>,
) -> JsValue {
    let mut items: Vec<JsValue> = Vec::new();
    loop {
        match lexer.peek_token() {
            Token::RBracket | Token::Eof => break,
            Token::Comma => { lexer.next_token(); items.push(JsValue::Undefined); }
            _ => {
                items.push(eval_expr_with_dom(lexer, scope, dom));
                if lexer.peek_token() == Token::Comma { lexer.next_token(); }
            }
        }
    }
    if lexer.peek_token() == Token::RBracket { lexer.next_token(); }
    JsValue::Array(std::sync::Arc::new(std::sync::Mutex::new(items)))
}

/// Evaluate an array method call on `arr` — lexer is at the token *after* the
/// method name has been consumed.  Returns the result value.
fn eval_array_method(
    lexer: &mut Lexer,
    arr:   &std::sync::Arc<std::sync::Mutex<Vec<JsValue>>>,
    prop:  &str,
) -> JsValue {
    // Property (no call): arr.length
    if prop == "length" && lexer.peek_token() != Token::LParen {
        return JsValue::Number(arr.lock().unwrap().len() as f64);
    }

    // Methods all require `(`
    if lexer.peek_token() != Token::LParen {
        // Unknown property
        return JsValue::Undefined;
    }

    // We need scope+dom for methods that take callbacks, but since
    // eval_array_method doesn't carry them we use a simpler path.
    // Simple methods first (no callbacks needed from this level).
    match prop {
        "length" => {
            // length called as method — skip parens and return length
            skip_call_args(lexer);
            JsValue::Number(arr.lock().unwrap().len() as f64)
        }
        "push" => {
            // Collect raw arg tokens — we can't easily re-eval without scope here.
            // Instead we return the array itself and let the statement-level handler
            // deal with it.  For now parse args via skip and return new length.
            // We actually want to push the value. Since we don't have scope here,
            // return a sentinel; the real push happens in run_statement.
            // Actually: we CAN'T eval args without scope. So this is a no-op.
            // The statement-level arr.push() is handled in the Ident branch.
            skip_call_args(lexer);
            JsValue::Number(arr.lock().unwrap().len() as f64)
        }
        "pop" => {
            skip_call_args(lexer);
            arr.lock().unwrap().pop().unwrap_or(JsValue::Undefined)
        }
        "join" => {
            skip_call_args(lexer);
            let sep = ",";
            let s = arr.lock().unwrap().iter().map(|v| v.to_display()).collect::<Vec<_>>().join(sep);
            JsValue::Str(s)
        }
        "indexOf" | "includes" | "slice" => {
            // Need args — skip for now
            skip_call_args(lexer);
            if prop == "includes" { JsValue::Bool(false) } else { JsValue::Number(-1.0) }
        }
        _ => {
            skip_call_args(lexer);
            JsValue::Undefined
        }
    }
}

/// Full array method evaluation WITH scope and dom available.
/// Called from eval_primary_dom when the leading ident is an Array variable.
fn eval_array_method_full(
    lexer: &mut Lexer,
    scope: &mut Scope,
    dom:   Option<&JsDom<'_>>,
    arr:   &std::sync::Arc<std::sync::Mutex<Vec<JsValue>>>,
    prop:  &str,
) -> JsValue {
    if prop == "length" && lexer.peek_token() != Token::LParen {
        return JsValue::Number(arr.lock().unwrap().len() as f64);
    }
    if lexer.peek_token() != Token::LParen {
        return JsValue::Undefined;
    }
    match prop {
        "length" => {
            skip_call_args(lexer);
            JsValue::Number(arr.lock().unwrap().len() as f64)
        }
        "push" => {
            lexer.next_token(); // consume `(`
            let mut new_len = arr.lock().unwrap().len();
            loop {
                match lexer.peek_token() {
                    Token::RParen | Token::Eof => break,
                    _ => {
                        let v = eval_expr_with_dom(lexer, scope, dom);
                        arr.lock().unwrap().push(v);
                        new_len = arr.lock().unwrap().len();
                        if lexer.peek_token() == Token::Comma { lexer.next_token(); }
                    }
                }
            }
            if lexer.peek_token() == Token::RParen { lexer.next_token(); }
            JsValue::Number(new_len as f64)
        }
        "pop" => {
            skip_call_args(lexer);
            arr.lock().unwrap().pop().unwrap_or(JsValue::Undefined)
        }
        "shift" => {
            skip_call_args(lexer);
            let mut v = arr.lock().unwrap();
            if v.is_empty() { JsValue::Undefined } else { v.remove(0) }
        }
        "unshift" => {
            lexer.next_token(); // `(`
            let mut items = Vec::new();
            loop {
                match lexer.peek_token() {
                    Token::RParen | Token::Eof => break,
                    _ => {
                        items.push(eval_expr_with_dom(lexer, scope, dom));
                        if lexer.peek_token() == Token::Comma { lexer.next_token(); }
                    }
                }
            }
            if lexer.peek_token() == Token::RParen { lexer.next_token(); }
            let mut v = arr.lock().unwrap();
            for item in items.into_iter().rev() { v.insert(0, item); }
            JsValue::Number(v.len() as f64)
        }
        "indexOf" => {
            lexer.next_token();
            let needle = eval_expr_with_dom(lexer, scope, dom);
            // skip optional fromIndex
            if lexer.peek_token() == Token::Comma { lexer.next_token(); eval_expr_with_dom(lexer, scope, dom); }
            if lexer.peek_token() == Token::RParen { lexer.next_token(); }
            let v = arr.lock().unwrap();
            for (i, item) in v.iter().enumerate() {
                if crate::js::eval::js_loose_eq(item, &needle) {
                    return JsValue::Number(i as f64);
                }
            }
            JsValue::Number(-1.0)
        }
        "includes" => {
            lexer.next_token();
            let needle = eval_expr_with_dom(lexer, scope, dom);
            if lexer.peek_token() == Token::Comma { lexer.next_token(); eval_expr_with_dom(lexer, scope, dom); }
            if lexer.peek_token() == Token::RParen { lexer.next_token(); }
            let v = arr.lock().unwrap();
            JsValue::Bool(v.iter().any(|item| crate::js::eval::js_loose_eq(item, &needle)))
        }
        "join" => {
            lexer.next_token();
            let sep = if matches!(lexer.peek_token(), Token::RParen | Token::Eof) {
                ",".to_owned()
            } else {
                let s = eval_expr_with_dom(lexer, scope, dom).to_display();
                s
            };
            if lexer.peek_token() == Token::RParen { lexer.next_token(); }
            let v = arr.lock().unwrap();
            JsValue::Str(v.iter().map(|x| x.to_display()).collect::<Vec<_>>().join(&sep))
        }
        "reverse" => {
            skip_call_args(lexer);
            arr.lock().unwrap().reverse();
            JsValue::Array(arr.clone())
        }
        "slice" => {
            lexer.next_token();
            let len = arr.lock().unwrap().len() as i64;
            let start_val = if matches!(lexer.peek_token(), Token::RParen | Token::Eof) {
                0
            } else {
                let n = eval_expr_with_dom(lexer, scope, dom).to_number() as i64;
                if n < 0 { (len + n).max(0) as usize } else { n.min(len) as usize }
            } as usize;
            let end_val = if lexer.peek_token() == Token::Comma {
                lexer.next_token();
                let n = eval_expr_with_dom(lexer, scope, dom).to_number() as i64;
                (if n < 0 { (len + n).max(0) } else { n.min(len) }) as usize
            } else {
                len as usize
            };
            if lexer.peek_token() == Token::RParen { lexer.next_token(); }
            let v = arr.lock().unwrap();
            let slice: Vec<JsValue> = v[start_val.min(v.len())..end_val.min(v.len())].to_vec();
            JsValue::Array(std::sync::Arc::new(std::sync::Mutex::new(slice)))
        }
        "concat" => {
            lexer.next_token();
            let mut result: Vec<JsValue> = arr.lock().unwrap().clone();
            loop {
                match lexer.peek_token() {
                    Token::RParen | Token::Eof => break,
                    _ => {
                        let v = eval_expr_with_dom(lexer, scope, dom);
                        match v {
                            JsValue::Array(other) => result.extend(other.lock().unwrap().iter().cloned()),
                            other => result.push(other),
                        }
                        if lexer.peek_token() == Token::Comma { lexer.next_token(); }
                    }
                }
            }
            if lexer.peek_token() == Token::RParen { lexer.next_token(); }
            JsValue::Array(std::sync::Arc::new(std::sync::Mutex::new(result)))
        }
        "forEach" => {
            let cb = { lexer.next_token(); let v = eval_expr_with_dom(lexer, scope, dom); if lexer.peek_token() == Token::RParen { lexer.next_token(); } v };
            if let JsValue::Function(func) = cb {
                let snapshot: Vec<JsValue> = arr.lock().unwrap().clone();
                for (i, item) in snapshot.into_iter().enumerate() {
                    let args = vec![item, JsValue::Number(i as f64)];
                    let mut dummy = Vec::new();
                    call_function(&func, args, &mut dummy, dom, scope, 0);
                    scope.entries.extend(dummy);
                }
            }
            JsValue::Undefined
        }
        "map" => {
            let cb = { lexer.next_token(); let v = eval_expr_with_dom(lexer, scope, dom); if lexer.peek_token() == Token::RParen { lexer.next_token(); } v };
            if let JsValue::Function(func) = cb {
                let snapshot: Vec<JsValue> = arr.lock().unwrap().clone();
                let mut out = Vec::new();
                for (i, item) in snapshot.into_iter().enumerate() {
                    let args = vec![item, JsValue::Number(i as f64)];
                    let mut dummy = Vec::new();
                    let r = call_function(&func, args, &mut dummy, dom, scope, 0);
                    scope.entries.extend(dummy);
                    out.push(r);
                }
                JsValue::Array(std::sync::Arc::new(std::sync::Mutex::new(out)))
            } else {
                JsValue::Undefined
            }
        }
        "filter" => {
            let cb = { lexer.next_token(); let v = eval_expr_with_dom(lexer, scope, dom); if lexer.peek_token() == Token::RParen { lexer.next_token(); } v };
            if let JsValue::Function(func) = cb {
                let snapshot: Vec<JsValue> = arr.lock().unwrap().clone();
                let mut out = Vec::new();
                for (i, item) in snapshot.into_iter().enumerate() {
                    let args = vec![item.clone(), JsValue::Number(i as f64)];
                    let mut dummy = Vec::new();
                    let r = call_function(&func, args, &mut dummy, dom, scope, 0);
                    scope.entries.extend(dummy);
                    if r.to_bool() { out.push(item); }
                }
                JsValue::Array(std::sync::Arc::new(std::sync::Mutex::new(out)))
            } else {
                JsValue::Undefined
            }
        }
        "find" => {
            let cb = { lexer.next_token(); let v = eval_expr_with_dom(lexer, scope, dom); if lexer.peek_token() == Token::RParen { lexer.next_token(); } v };
            if let JsValue::Function(func) = cb {
                let snapshot: Vec<JsValue> = arr.lock().unwrap().clone();
                for (i, item) in snapshot.into_iter().enumerate() {
                    let args = vec![item.clone(), JsValue::Number(i as f64)];
                    let mut dummy = Vec::new();
                    let r = call_function(&func, args, &mut dummy, dom, scope, 0);
                    scope.entries.extend(dummy);
                    if r.to_bool() { return item; }
                }
                JsValue::Undefined
            } else {
                JsValue::Undefined
            }
        }
        "reduce" => {
            lexer.next_token();
            let cb = eval_expr_with_dom(lexer, scope, dom);
            let mut acc = if lexer.peek_token() == Token::Comma {
                lexer.next_token();
                eval_expr_with_dom(lexer, scope, dom)
            } else {
                arr.lock().unwrap().first().cloned().unwrap_or(JsValue::Undefined)
            };
            if lexer.peek_token() == Token::RParen { lexer.next_token(); }
            if let JsValue::Function(func) = cb {
                let snapshot: Vec<JsValue> = arr.lock().unwrap().clone();
                for (i, item) in snapshot.into_iter().enumerate() {
                    let args = vec![acc, item, JsValue::Number(i as f64)];
                    let mut dummy = Vec::new();
                    acc = call_function(&func, args, &mut dummy, dom, scope, 0);
                    scope.entries.extend(dummy);
                }
                acc
            } else {
                acc
            }
        }
        "some" => {
            let cb = { lexer.next_token(); let v = eval_expr_with_dom(lexer, scope, dom); if lexer.peek_token() == Token::RParen { lexer.next_token(); } v };
            if let JsValue::Function(func) = cb {
                let snapshot: Vec<JsValue> = arr.lock().unwrap().clone();
                for (i, item) in snapshot.into_iter().enumerate() {
                    let args = vec![item, JsValue::Number(i as f64)];
                    let mut dummy = Vec::new();
                    let r = call_function(&func, args, &mut dummy, dom, scope, 0);
                    scope.entries.extend(dummy);
                    if r.to_bool() { return JsValue::Bool(true); }
                }
                JsValue::Bool(false)
            } else {
                JsValue::Bool(false)
            }
        }
        "every" => {
            let cb = { lexer.next_token(); let v = eval_expr_with_dom(lexer, scope, dom); if lexer.peek_token() == Token::RParen { lexer.next_token(); } v };
            if let JsValue::Function(func) = cb {
                let snapshot: Vec<JsValue> = arr.lock().unwrap().clone();
                for (i, item) in snapshot.into_iter().enumerate() {
                    let args = vec![item, JsValue::Number(i as f64)];
                    let mut dummy = Vec::new();
                    let r = call_function(&func, args, &mut dummy, dom, scope, 0);
                    scope.entries.extend(dummy);
                    if !r.to_bool() { return JsValue::Bool(false); }
                }
                JsValue::Bool(true)
            } else {
                JsValue::Bool(true)
            }
        }
        _ => {
            skip_call_args(lexer);
            JsValue::Undefined
        }
    }
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
        Token::Bang      => { lexer.next_token(); let v = eval_unary_dom(lexer, scope, dom); JsValue::Bool(!v.to_bool()) }
        Token::Minus     => { lexer.next_token(); let v = eval_unary_dom(lexer, scope, dom); JsValue::Number(-v.to_number()) }
        // Prefix ++ / -- in expression context
        Token::PlusPlus  => {
            lexer.next_token();
            if let Token::Ident(name) = lexer.peek_token() {
                lexer.next_token();
                let new = scope.get(&name).to_number() + 1.0;
                scope.set(&name, JsValue::Number(new));
                JsValue::Number(new)
            } else { JsValue::Undefined }
        }
        Token::MinusMinus => {
            lexer.next_token();
            if let Token::Ident(name) = lexer.peek_token() {
                lexer.next_token();
                let new = scope.get(&name).to_number() - 1.0;
                scope.set(&name, JsValue::Number(new));
                JsValue::Number(new)
            } else { JsValue::Undefined }
        }
        _ => eval_postfix_dom(lexer, scope, dom),
    }
}

fn eval_postfix_dom(lexer: &mut Lexer, scope: &mut Scope, dom: Option<&JsDom<'_>>) -> JsValue {
    let mut val = eval_primary_dom(lexer, scope, dom);
    loop {
        match lexer.peek_token() {
            // Postfix ++ / --
            Token::PlusPlus => {
                lexer.next_token();
                // We can't easily get the name back from `val`, so we do a best-effort:
                // the caller already read the name in eval_primary_dom, but we lost it.
                // Instead we return the old value (correct) and don't mutate (limitation).
                // Full fix would require a "lvalue" path — acceptable trade-off for now.
                break;
            }
            Token::MinusMinus => {
                lexer.next_token();
                break;
            }
            // Subscript access: arr[index]
            Token::LBracket => {
                lexer.next_token(); // consume `[`
                let idx = eval_expr_with_dom(lexer, scope, dom);
                if lexer.peek_token() == Token::RBracket { lexer.next_token(); }
                val = match &val {
                    JsValue::Array(arr) => {
                        let i = idx.to_number();
                        if i >= 0.0 && i.fract() == 0.0 {
                            arr.lock().unwrap().get(i as usize).cloned().unwrap_or(JsValue::Undefined)
                        } else {
                            JsValue::Undefined
                        }
                    }
                    JsValue::Str(s) => {
                        // string[n] — return character
                        let i = idx.to_number();
                        if i >= 0.0 && i.fract() == 0.0 {
                            s.chars().nth(i as usize)
                                .map(|c| JsValue::Str(c.to_string()))
                                .unwrap_or(JsValue::Undefined)
                        } else {
                            JsValue::Undefined
                        }
                    }
                    _ => JsValue::Undefined,
                };
            }
            // Dot access on array: arr.push / arr.length etc. — handled via chain_with_scope
            Token::Dot => {
                if let JsValue::Array(_) = &val {
                    val = chain_with_scope(lexer, val, scope, dom);
                    break;
                }
                break;
            }
            _ => break,
        }
    }
    val
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
            // Anonymous function expression: `function(...) { ... }`
            "function"  => {
                let func = parse_function_value(lexer);
                JsValue::Function(Box::new(func))
            }
            name => {
                let val = scope.get(name);
                // Dot-chain: property access on DOM elements / lists / arrays
                if lexer.peek_token() == Token::Dot {
                    if let JsValue::Array(ref arr) = val {
                        lexer.next_token(); // consume `.`
                        let prop = match lexer.next_token() {
                            Token::Ident(p) => p,
                            _ => return JsValue::Undefined,
                        };
                        let result = eval_array_method_full(lexer, scope, dom, arr, &prop);
                        // Continue chaining (e.g. arr.map(...).join(...))
                        return chain_with_scope(lexer, result, scope, dom);
                    }
                    if let JsValue::Str(ref s) = val {
                        if s.starts_with("\x00elem\x00") || s.starts_with("\x00list\x00") {
                            return chain_props_inner(lexer, val, dom);
                        }
                    }
                    // Unknown object — skip the dot-chain
                    crate::js::eval::skip_dot_call_chain(lexer);
                    return JsValue::Undefined;
                }
                // Subscript access: name[index]
                if lexer.peek_token() == Token::LBracket {
                    if let JsValue::Array(ref arr) = val {
                        lexer.next_token(); // consume `[`
                        let idx = eval_expr_with_dom(lexer, scope, dom);
                        if lexer.peek_token() == Token::RBracket { lexer.next_token(); }
                        let i = idx.to_number();
                        let item = if i >= 0.0 && i.fract() == 0.0 {
                            arr.lock().unwrap().get(i as usize).cloned().unwrap_or(JsValue::Undefined)
                        } else {
                            JsValue::Undefined
                        };
                        // Allow chaining after subscript: arr[i].prop
                        return chain_with_scope(lexer, item, scope, dom);
                    }
                }
                // Function call: `foo(...)`
                if lexer.peek_token() == Token::LParen {
                    if let JsValue::Function(ref func) = val {
                        let func = func.as_ref().clone();
                        let args = eval_arg_list(lexer, scope, dom);
                        let mut dummy = Vec::new();
                        return call_function(&func, args, &mut dummy, dom, scope, 0);
                    }
                    // Not a function — skip args
                    skip_call_args(lexer);
                    return JsValue::Undefined;
                }
                val
            }
        },
        Token::LParen => {
            // `(` was already consumed.
            // Could be a grouped expr `(expr)` OR an arrow param list `(x) => ...`
            // Check from the current position (already past `(`).
            let saved = lexer.pos;
            if is_arrow_after_open_paren(lexer) {
                // It's `(params) => body`.  Re-parse the param list from saved pos.
                lexer.pos = saved;
                let params = parse_param_list_after_open(lexer);
                let func = parse_arrow_body(lexer, params);
                return JsValue::Function(Box::new(func));
            }
            lexer.pos = saved;
            let inner = eval_expr_with_dom(lexer, scope, dom);
            if lexer.peek_token() == Token::RParen { lexer.next_token(); }
            inner
        }
        // Array literal — parse elements into a JsValue::Array
        Token::LBracket => {
            eval_array_literal(lexer, scope, dom)
        }
        // Object literal — skip and return Undefined
        Token::LBrace => {
            skip_block(lexer);
            JsValue::Undefined
        }
        _ => JsValue::Undefined,
    }
}

/// Evaluate a `(arg1, arg2, ...)` argument list, consuming the parens.
fn eval_arg_list(
    lexer: &mut Lexer,
    scope: &mut Scope,
    dom:   Option<&JsDom<'_>>,
) -> Vec<JsValue> {
    let mut args = Vec::new();
    if lexer.peek_token() != Token::LParen { return args; }
    lexer.next_token(); // consume `(`
    loop {
        match lexer.peek_token() {
            Token::RParen | Token::Eof => break,
            _ => {
                args.push(eval_expr_with_dom(lexer, scope, dom));
                if lexer.peek_token() == Token::Comma { lexer.next_token(); }
            }
        }
    }
    if lexer.peek_token() == Token::RParen { lexer.next_token(); }
    args
}

/// Check if we are positioned just AFTER an already-consumed `(` and the
/// content forms an arrow function param list followed by `=>`.
/// Scans to the matching `)` then looks for `=>`.
/// Does NOT consume tokens permanently — restores `lexer.pos` on return.
fn is_arrow_after_open_paren(lexer: &mut Lexer) -> bool {
    let saved = lexer.pos;
    let mut depth = 1i32;
    loop {
        match lexer.next_token() {
            Token::Eof => { lexer.pos = saved; return false; }
            Token::LParen => depth += 1,
            Token::RParen => {
                depth -= 1;
                if depth == 0 { break; }
            }
            _ => {}
        }
    }
    // Now check for `=>`
    let result = lexer.peek_token() == Token::Eq && {
        let s2 = lexer.pos;
        lexer.next_token();
        let is_gt = lexer.peek_token() == Token::Gt;
        lexer.pos = s2;
        is_gt
    };
    lexer.pos = saved;
    result
}

/// Parse `a, b, c)` (the `(` has already been consumed) — returns param names.
fn parse_param_list_after_open(lexer: &mut Lexer) -> Vec<String> {
    let mut params = Vec::new();
    loop {
        match lexer.peek_token() {
            Token::RParen | Token::Eof => break,
            Token::Ident(_) => {
                if let Token::Ident(p) = lexer.next_token() { params.push(p); }
                if lexer.peek_token() == Token::Comma { lexer.next_token(); }
            }
            _ => { lexer.next_token(); }
        }
    }
    if lexer.peek_token() == Token::RParen { lexer.next_token(); }
    params
}

/// Parse `=> expr` or `=> { body }` — the `(params)` has already been consumed.
fn parse_arrow_body(lexer: &mut Lexer, params: Vec<String>) -> crate::js::types::JsFunction {
    use crate::js::types::JsFunction;
    // consume `=>`
    if lexer.peek_token() == Token::Eq {
        lexer.next_token();
        if lexer.peek_token() == Token::Gt { lexer.next_token(); }
    }
    if lexer.peek_token() == Token::LBrace {
        let body = capture_block_src(lexer);
        JsFunction { params, body, is_expr_body: false }
    } else {
        let body = capture_expr_src(lexer);
        JsFunction { params, body, is_expr_body: true }
    }
}

/// Quick check: is the lexer (currently past an already-consumed `(`) about
/// to see a param list followed by `=>`?  We scan to the matching `)` and
/// check the next two chars.  Does not consume on success/failure — caller
/// must restore `lexer.pos`.
fn is_arrow_params(lexer: &mut Lexer) -> bool {
    // We are positioned just AFTER the `(` was consumed (called from LParen arm
    // where we saved pos *before* calling this, so we start at `(`).
    // Actually we saved pos before the match, so `(` is still at pos.
    // Skip `(` first.
    if lexer.peek_token() != Token::LParen { return false; }
    let saved = lexer.pos;
    lexer.next_token(); // consume `(`
    let mut depth = 1i32;
    loop {
        match lexer.next_token() {
            Token::Eof => { lexer.pos = saved; return false; }
            Token::LParen => depth += 1,
            Token::RParen => {
                depth -= 1;
                if depth == 0 { break; }
            }
            _ => {}
        }
    }
    // Now check for `=>`
    let result = lexer.peek_token() == Token::Eq && {
        let s2 = lexer.pos;
        lexer.next_token();
        let is_gt = lexer.peek_token() == Token::Gt;
        lexer.pos = s2;
        is_gt
    };
    lexer.pos = saved;
    result
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

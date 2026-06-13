use crate::dom::node::BoxSpacing;

/// Context needed to resolve CSS length values to pixels.
#[derive(Debug, Clone, Copy)]
pub struct LengthContext {
    pub base_font_size:  u16,
    pub percent_base:    i32,
    pub viewport_width:  i32,
    pub viewport_height: i32,
}

impl Default for LengthContext {
    fn default() -> Self {
        LengthContext {
            base_font_size:  16,
            percent_base:    0,
            viewport_width:  800,
            viewport_height: 600,
        }
    }
}

/// Parse a CSS length value to pixels using a full `LengthContext`.
/// Handles: px, pt, em, rem, %, vw, vh, ch, ex, bare numbers,
/// and the math functions calc(), clamp(), min(), max().
pub fn parse_length_ctx(val: &str, ctx: &LengthContext) -> Option<i32> {
    let v = val.trim();
    if v.is_empty() { return None; }
    if v == "0" { return Some(0); }

    // ---- math functions ----
    let lower = v.to_ascii_lowercase();
    if lower.starts_with("calc(") && lower.ends_with(')') {
        let inner = &v[5..v.len()-1];
        return eval_calc(inner, ctx);
    }
    if lower.starts_with("clamp(") && lower.ends_with(')') {
        let inner = &v[6..v.len()-1];
        return eval_clamp(inner, ctx);
    }
    if lower.starts_with("min(") && lower.ends_with(')') {
        let inner = &v[4..v.len()-1];
        return eval_min(inner, ctx);
    }
    if lower.starts_with("max(") && lower.ends_with(')') {
        let inner = &v[4..v.len()-1];
        return eval_max(inner, ctx);
    }

    // ---- units ----
    if let Some(n) = v.strip_suffix("px") {
        return n.trim().parse::<f32>().ok().map(|n| n as i32);
    }
    if let Some(n) = v.strip_suffix("pt") {
        return n.trim().parse::<f32>().ok().map(|n| (n * 1.333) as i32);
    }
    if let Some(n) = v.strip_suffix("rem").or_else(|| v.strip_suffix("em")) {
        return n.trim().parse::<f32>().ok()
            .map(|n| (n * ctx.base_font_size as f32).round() as i32);
    }
    if let Some(n) = v.strip_suffix("vw") {
        return n.trim().parse::<f32>().ok()
            .map(|n| (n / 100.0 * ctx.viewport_width as f32).round() as i32);
    }
    if let Some(n) = v.strip_suffix("vh") {
        return n.trim().parse::<f32>().ok()
            .map(|n| (n / 100.0 * ctx.viewport_height as f32).round() as i32);
    }
    // ch ≈ 0.5em, ex ≈ 0.5em (approximation when font metrics unavailable)
    if let Some(n) = v.strip_suffix("ch").or_else(|| v.strip_suffix("ex")) {
        return n.trim().parse::<f32>().ok()
            .map(|n| (n * ctx.base_font_size as f32 * 0.5).round() as i32);
    }
    if let Some(n) = v.strip_suffix('%') {
        return n.trim().parse::<f32>().ok()
            .map(|n| (n / 100.0 * ctx.percent_base as f32).round() as i32);
    }
    v.parse::<f32>().ok().map(|n| n as i32)
}

/// Backward-compatible wrapper — constructs a default LengthContext with
/// the supplied font size and percent base, viewport 800×600.
pub fn parse_length(val: &str, base_font_size: u16, percent_base: i32) -> Option<i32> {
    parse_length_ctx(val, &LengthContext {
        base_font_size,
        percent_base,
        viewport_width:  800,
        viewport_height: 600,
    })
}

// ---------------------------------------------------------------------------
// Math function evaluators
// ---------------------------------------------------------------------------

/// Evaluate a `calc(...)` expression.
/// Supports +, -, *, / with standard operator precedence.
pub fn eval_calc(expr: &str, ctx: &LengthContext) -> Option<i32> {
    eval_expr(expr.trim(), ctx)
}

/// Evaluate `clamp(min, preferred, max)` → clamps preferred between min and max.
pub fn eval_clamp(args: &str, ctx: &LengthContext) -> Option<i32> {
    let parts = split_args(args);
    if parts.len() != 3 { return None; }
    let mn  = parse_length_ctx(parts[0].trim(), ctx)?;
    let val = parse_length_ctx(parts[1].trim(), ctx)?;
    let mx  = parse_length_ctx(parts[2].trim(), ctx)?;
    Some(val.max(mn).min(mx))
}

/// Evaluate `min(a, b)` → smaller of the two.
pub fn eval_min(args: &str, ctx: &LengthContext) -> Option<i32> {
    let parts = split_args(args);
    if parts.len() != 2 { return None; }
    let a = parse_length_ctx(parts[0].trim(), ctx)?;
    let b = parse_length_ctx(parts[1].trim(), ctx)?;
    Some(a.min(b))
}

/// Evaluate `max(a, b)` → larger of the two.
pub fn eval_max(args: &str, ctx: &LengthContext) -> Option<i32> {
    let parts = split_args(args);
    if parts.len() != 2 { return None; }
    let a = parse_length_ctx(parts[0].trim(), ctx)?;
    let b = parse_length_ctx(parts[1].trim(), ctx)?;
    Some(a.max(b))
}

// ---------------------------------------------------------------------------
// Expression parser (simple recursive descent for +/-/*//)
// ---------------------------------------------------------------------------

/// Split comma-separated arguments, respecting nested parentheses.
fn split_args(s: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut depth  = 0usize;
    let mut start  = 0usize;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => { if depth > 0 { depth -= 1; } }
            ',' if depth == 0 => {
                result.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    result.push(&s[start..]);
    result
}

/// Evaluate an arithmetic expression of CSS length terms.
/// Handles +, -, *, / with correct precedence via two-pass approach.
fn eval_expr(expr: &str, ctx: &LengthContext) -> Option<i32> {
    // Tokenise into terms and operators at the top level (depth 0).
    let tokens = tokenise_expr(expr);
    if tokens.is_empty() { return None; }

    // First pass: handle * and /
    let mut intermediate: Vec<Token> = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        match &tokens[i] {
            Token::Op('*') => {
                let lhs = intermediate.pop()?.as_value(ctx)?;
                i += 1;
                let rhs = tokens[i].as_value(ctx)?;
                intermediate.push(Token::Value((lhs * rhs).round() as i32));
            }
            Token::Op('/') => {
                let lhs = intermediate.pop()?.as_value(ctx)?;
                i += 1;
                let rhs_f = tokens[i].as_value_f(ctx)?;
                if rhs_f == 0.0 {
                    eprintln!("calc(): division by zero, treating as 0");
                    intermediate.push(Token::Value(0));
                } else {
                    intermediate.push(Token::Value((lhs as f32 / rhs_f).round() as i32));
                }
            }
            t => intermediate.push(t.clone()),
        }
        i += 1;
    }

    // Second pass: handle + and -
    let mut result: f32 = 0.0;
    let mut op = '+';
    for tok in &intermediate {
        match tok {
            Token::Op(c) => op = *c,
            _ => {
                let v = tok.as_value_f(ctx)?;
                match op {
                    '+' => result += v,
                    '-' => result -= v,
                    _   => {}
                }
            }
        }
    }
    Some(result.ceil() as i32)
}

#[derive(Clone, Debug)]
enum Token<'a> {
    Term(&'a str),
    Value(i32),
    Op(char),
}

impl<'a> Token<'a> {
    fn as_value(&self, ctx: &LengthContext) -> Option<f32> {
        self.as_value_f(ctx)
    }
    fn as_value_f(&self, ctx: &LengthContext) -> Option<f32> {
        match self {
            Token::Value(n) => Some(*n as f32),
            Token::Term(s)  => parse_length_ctx(s.trim(), ctx).map(|n| n as f32),
            Token::Op(_)    => None,
        }
    }
}

/// Tokenise a calc expression into terms and operators at depth 0.
fn tokenise_expr(expr: &str) -> Vec<Token<'_>> {
    let mut tokens: Vec<Token<'_>> = Vec::new();
    let bytes = expr.as_bytes();
    let len   = bytes.len();
    let mut i = 0usize;
    let mut depth = 0usize;
    let mut term_start: Option<usize> = None;

    macro_rules! flush {
        ($start:expr, $end:expr) => {{
            let s = expr[$start..$end].trim();
            if !s.is_empty() { tokens.push(Token::Term(s)); }
        }};
    }

    while i < len {
        let c = bytes[i] as char;
        match c {
            '(' => { depth += 1; if term_start.is_none() { term_start = Some(i); } }
            ')' => { if depth > 0 { depth -= 1; } }
            '+' | '-' if depth == 0 && i > 0 => {
                // Make sure it's not a sign at the start
                let prev = bytes[i - 1] as char;
                if prev == ' ' || prev == '\t' {
                    if let Some(s) = term_start.take() {
                        flush!(s, i);
                    }
                    tokens.push(Token::Op(c));
                    i += 1;
                    continue;
                }
            }
            '*' | '/' if depth == 0 => {
                if let Some(s) = term_start.take() {
                    flush!(s, i);
                }
                tokens.push(Token::Op(c));
                i += 1;
                continue;
            }
            _ => {}
        }
        if term_start.is_none() && c != ' ' && c != '\t' {
            term_start = Some(i);
        }
        i += 1;
    }
    if let Some(s) = term_start {
        flush!(s, len);
    }
    tokens
}

// ---------------------------------------------------------------------------
// Box spacing (unchanged public API)
// ---------------------------------------------------------------------------

/// Parse a CSS shorthand spacing value (margin/padding) into a `BoxSpacing`.
pub fn parse_box_spacing(val: &str, base_font: u16) -> BoxSpacing {
    let parts: Vec<&str> = val.split_whitespace().collect();
    let px = |s: &str| parse_length(s, base_font, 0).unwrap_or(0);
    match parts.len() {
        1 => { let v = px(parts[0]); BoxSpacing { top: v, right: v, bottom: v, left: v } }
        2 => BoxSpacing { top: px(parts[0]), right: px(parts[1]), bottom: px(parts[0]), left: px(parts[1]) },
        3 => BoxSpacing { top: px(parts[0]), right: px(parts[1]), bottom: px(parts[2]), left: px(parts[1]) },
        4 => BoxSpacing { top: px(parts[0]), right: px(parts[1]), bottom: px(parts[2]), left: px(parts[3]) },
        _ => BoxSpacing::default(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> LengthContext {
        LengthContext {
            base_font_size:  16,
            percent_base:    800,
            viewport_width:  1000,
            viewport_height: 600,
        }
    }

    #[test]
    fn px_unit() {
        assert_eq!(parse_length_ctx("24px", &ctx()), Some(24));
    }

    #[test]
    fn vw_unit() {
        // 10vw of 1000px viewport = 100px
        assert_eq!(parse_length_ctx("10vw", &ctx()), Some(100));
    }

    #[test]
    fn vh_unit() {
        // 50vh of 600px viewport = 300px
        assert_eq!(parse_length_ctx("50vh", &ctx()), Some(300));
    }

    #[test]
    fn ch_unit_fallback() {
        // 2ch = 2 * 0.5 * 16 = 16px
        assert_eq!(parse_length_ctx("2ch", &ctx()), Some(16));
    }

    #[test]
    fn calc_add() {
        assert_eq!(parse_length_ctx("calc(10px + 5px)", &ctx()), Some(15));
    }

    #[test]
    fn calc_subtract() {
        assert_eq!(parse_length_ctx("calc(20px - 4px)", &ctx()), Some(16));
    }

    #[test]
    fn calc_multiply() {
        assert_eq!(parse_length_ctx("calc(3px * 4)", &ctx()), Some(12));
    }

    #[test]
    fn calc_divide() {
        assert_eq!(parse_length_ctx("calc(20px / 4)", &ctx()), Some(5));
    }

    #[test]
    fn calc_divide_by_zero() {
        assert_eq!(parse_length_ctx("calc(10px / 0)", &ctx()), Some(0));
    }

    #[test]
    fn clamp_in_range() {
        // clamp(10px, 50px, 100px) -> 50
        assert_eq!(parse_length_ctx("clamp(10px, 50px, 100px)", &ctx()), Some(50));
    }

    #[test]
    fn clamp_below_min() {
        // clamp(10px, 5px, 100px) -> 10
        assert_eq!(parse_length_ctx("clamp(10px, 5px, 100px)", &ctx()), Some(10));
    }

    #[test]
    fn clamp_above_max() {
        // clamp(10px, 50px, 20px) -> 20
        assert_eq!(parse_length_ctx("clamp(10px, 50px, 20px)", &ctx()), Some(20));
    }

    #[test]
    fn min_fn() {
        assert_eq!(parse_length_ctx("min(30px, 50px)", &ctx()), Some(30));
    }

    #[test]
    fn max_fn() {
        assert_eq!(parse_length_ctx("max(30px, 50px)", &ctx()), Some(50));
    }

    #[test]
    fn round_trip_px() {
        // parse "42px" -> 42, format back -> "42px" -> parse -> 42
        let n = parse_length_ctx("42px", &ctx()).unwrap();
        let s = format!("{}px", n);
        assert_eq!(parse_length_ctx(&s, &ctx()), Some(42));
    }
}

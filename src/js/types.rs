/// Core JS value type and coercions.

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
    pub fn to_number(&self) -> f64 {
        match self {
            JsValue::Number(n)  => *n,
            JsValue::Bool(b)    => if *b { 1.0 } else { 0.0 },
            JsValue::Str(s)     => s.trim().parse::<f64>().unwrap_or(f64::NAN),
            JsValue::Null       => 0.0,
            JsValue::Undefined  => f64::NAN,
        }
    }

    /// Coerce to bool (JS truthy semantics).
    pub fn to_bool(&self) -> bool {
        match self {
            JsValue::Bool(b)   => *b,
            JsValue::Number(n) => *n != 0.0 && !n.is_nan(),
            JsValue::Str(s)    => !s.is_empty(),
            JsValue::Null      => false,
            JsValue::Undefined => false,
        }
    }
}

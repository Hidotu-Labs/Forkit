/// Flat variable store for the current script execution.

use std::collections::HashMap;
use crate::js::types::JsValue;

pub struct Scope {
    vars: HashMap<String, JsValue>,
}

impl Scope {
    pub fn new() -> Self {
        Scope { vars: HashMap::new() }
    }

    pub fn set(&mut self, name: &str, val: JsValue) {
        self.vars.insert(name.to_owned(), val);
    }

    pub fn get(&self, name: &str) -> JsValue {
        self.vars.get(name).cloned().unwrap_or(JsValue::Undefined)
    }
}

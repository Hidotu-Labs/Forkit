/// Variable store for script execution.
///
/// Maintains a stack of frames so that block-scoped `let`/`const` bindings
/// introduced inside `if`/`while`/`for` bodies are visible inside that block
/// but not after it.  `var` declarations use the same stack but are treated
/// identically here — hoisting is not implemented.

use std::collections::HashMap;
use crate::js::types::JsValue;
use crate::js::console::ConsoleEntry;

pub struct Scope {
    /// Stack of frames; the last entry is the innermost (current) scope.
    /// There is always at least one frame (the global frame).
    frames: Vec<HashMap<String, JsValue>>,
    /// Console entries produced during execution (including inside function calls).
    /// Shared across all call frames.
    pub entries: Vec<ConsoleEntry>,
    /// Current call depth — used to enforce the recursion limit.
    pub call_depth: usize,
}

impl Scope {
    pub fn new() -> Self {
        Scope { frames: vec![HashMap::new()], entries: Vec::new(), call_depth: 0 }
    }

    /// Push a new block scope frame.
    pub fn push(&mut self) {
        self.frames.push(HashMap::new());
    }

    /// Pop the innermost block scope frame.
    pub fn pop(&mut self) {
        if self.frames.len() > 1 {
            self.frames.pop();
        }
    }

    /// Set a variable.  If the name already exists in any enclosing frame,
    /// update it there (assignment semantics).  Otherwise create it in the
    /// current (innermost) frame (declaration semantics).
    pub fn set(&mut self, name: &str, val: JsValue) {
        // Walk frames from innermost to outermost looking for an existing binding.
        for frame in self.frames.iter_mut().rev() {
            if frame.contains_key(name) {
                frame.insert(name.to_owned(), val);
                return;
            }
        }
        // Not found — create in current frame.
        if let Some(frame) = self.frames.last_mut() {
            frame.insert(name.to_owned(), val);
        }
    }

    /// Declare a new binding in the current (innermost) frame regardless of
    /// whether a same-named binding exists in an outer frame.
    pub fn declare(&mut self, name: &str, val: JsValue) {
        if let Some(frame) = self.frames.last_mut() {
            frame.insert(name.to_owned(), val);
        }
    }

    /// Look up a variable, searching from innermost to outermost frame.
    pub fn get(&self, name: &str) -> JsValue {
        for frame in self.frames.iter().rev() {
            if let Some(v) = frame.get(name) {
                return v.clone();
            }
        }
        JsValue::Undefined
    }
}

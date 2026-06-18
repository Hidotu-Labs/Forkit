/// JavaScript engine — public API.
///
/// Modules:
///   types       — JsValue and coercions
///   scope       — flat variable store
///   lexer       — Token + Lexer
///   eval        — shared helpers (js_loose_eq, skip_*)
///   console     — ConsoleEntry / ConsoleLevel types
///   dom         — read-only DOM view (JsDom, JsElement)
///   interpreter — statement runner + execute() / execute_with_dom()

pub mod types;
pub mod scope;
pub mod lexer;
pub mod eval;
pub mod console;
pub mod dom;
pub mod interpreter;

// Re-export the public surface used by the rest of the codebase.
pub use console::{ConsoleEntry, ConsoleLevel};
pub use interpreter::{execute, execute_with_dom};
pub use dom::{JsDom, apply_mutations, apply_one, DomMutation};

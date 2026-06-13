/// Public console output types produced by script execution.

/// Log level emitted by a `console.*` call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleLevel {
    Log,
    Warn,
    Error,
}

/// A single console output entry produced by script execution.
#[derive(Debug, Clone)]
pub struct ConsoleEntry {
    pub level:   ConsoleLevel,
    pub message: String,
}

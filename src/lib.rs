#![doc = include_str!("../README.md")]

pub mod ast;
pub mod error;
pub mod interpreter;
pub mod lexer;
pub mod parser;

pub use error::{LumenError, LumenRuntimeError, LumenSyntaxError};
pub use interpreter::{Interpreter, Value, MAX_CALL_DEPTH};
pub use lexer::{tokenize, Tok, Token};
pub use parser::parse;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Run `f` on a thread with a 256 MiB stack and return its result.
///
/// The interpreter recurses on the native stack, so `MAX_CALL_DEPTH` nested
/// Lumen calls do not fit in a default 2-8 MiB stack. The `lumen` binary runs
/// everything through this; embedders running deep recursion should too.
pub fn run_on_big_stack<T: Send>(f: impl FnOnce() -> T + Send) -> T {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(256 * 1024 * 1024)
            .spawn_scoped(scope, f)
            .expect("failed to spawn interpreter thread")
            .join()
            .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
    })
}

/// REPL state: buffers lines until `{` and `(` are balanced, then runs them.
pub struct Session {
    interp: Interpreter,
    buf: String,
}

impl Default for Session {
    fn default() -> Self {
        Self::new(Interpreter::new())
    }
}

impl Session {
    pub fn new(interp: Interpreter) -> Self {
        Session { interp, buf: String::new() }
    }

    /// True while a multi-line entry is in progress.
    pub fn pending(&self) -> bool {
        !self.buf.is_empty()
    }

    /// Discard a half-typed entry.
    pub fn reset(&mut self) {
        self.buf.clear();
    }

    /// Add one line. Returns `None` while brackets are still open, otherwise
    /// the result of running the whole entry.
    pub fn feed(&mut self, line: &str) -> Option<Result<Option<Value>, LumenError>> {
        self.buf.push_str(line);
        self.buf.push('\n');
        if let Ok(tokens) = tokenize(&self.buf) {
            let depth: i32 = tokens
                .iter()
                .map(|t| match t.tok {
                    Tok::Sym("{") | Tok::Sym("(") => 1,
                    Tok::Sym("}") | Tok::Sym(")") => -1,
                    _ => 0,
                })
                .sum();
            if depth > 0 {
                return None;
            }
        }
        let source = std::mem::take(&mut self.buf);
        Some(self.interp.run(&source))
    }
}

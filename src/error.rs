use std::fmt;

/// Raised by the lexer and parser.
#[derive(Debug, Clone, PartialEq)]
pub struct LumenSyntaxError {
    pub line: usize,
    pub message: String,
}

/// Raised while a program is running.
#[derive(Debug, Clone, PartialEq)]
pub struct LumenRuntimeError {
    pub line: usize,
    pub message: String,
}

/// Either kind of error, as returned by `Interpreter::run`.
#[derive(Debug, Clone, PartialEq)]
pub enum LumenError {
    Syntax(LumenSyntaxError),
    Runtime(LumenRuntimeError),
}

impl LumenSyntaxError {
    pub fn new(line: usize, message: impl Into<String>) -> Self {
        Self { line, message: message.into() }
    }
}

impl LumenRuntimeError {
    pub fn new(line: usize, message: impl Into<String>) -> Self {
        Self { line, message: message.into() }
    }
}

impl LumenError {
    pub fn line(&self) -> usize {
        match self {
            Self::Syntax(e) => e.line,
            Self::Runtime(e) => e.line,
        }
    }

    pub fn message(&self) -> &str {
        match self {
            Self::Syntax(e) => &e.message,
            Self::Runtime(e) => &e.message,
        }
    }
}

impl fmt::Display for LumenSyntaxError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "[line {}] SyntaxError: {}", self.line, self.message)
    }
}

impl fmt::Display for LumenRuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "[line {}] RuntimeError: {}", self.line, self.message)
    }
}

impl fmt::Display for LumenError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Syntax(e) => e.fmt(f),
            Self::Runtime(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for LumenSyntaxError {}
impl std::error::Error for LumenRuntimeError {}
impl std::error::Error for LumenError {}

impl From<LumenSyntaxError> for LumenError {
    fn from(e: LumenSyntaxError) -> Self {
        Self::Syntax(e)
    }
}

impl From<LumenRuntimeError> for LumenError {
    fn from(e: LumenRuntimeError) -> Self {
        Self::Runtime(e)
    }
}

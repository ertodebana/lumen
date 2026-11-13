use std::rc::Rc;

/// Expressions carry a `line` wherever a runtime error can originate.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Num(f64),
    Str(Rc<str>),
    Bool(bool),
    Nil,
    Var { name: String, line: usize },
    Assign { name: String, value: Box<Expr>, line: usize },
    Neg { operand: Box<Expr>, line: usize },
    Not(Box<Expr>),
    Binary { op: &'static str, left: Box<Expr>, right: Box<Expr>, line: usize },
    /// Short-circuiting `and` / `or`.
    Logic { or: bool, left: Box<Expr>, right: Box<Expr> },
    Call { callee: Box<Expr>, args: Vec<Expr>, line: usize },
    Fn(Rc<FnDecl>),
}

#[derive(Debug, PartialEq)]
pub struct FnDecl {
    pub name: Option<String>,
    pub params: Vec<String>,
    pub body: Vec<Stmt>,
}

/// `fn name(...) {...}` is parsed as `Let(name, Some(Expr::Fn(..)))`.
#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Let(String, Option<Expr>),
    Print(Expr),
    /// The else branch is either another `If` or a `Block`.
    If(Expr, Vec<Stmt>, Option<Box<Stmt>>),
    While(Expr, Vec<Stmt>),
    Return(Option<Expr>),
    Block(Vec<Stmt>),
    Expr(Expr),
}

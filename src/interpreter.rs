use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::ast::{Expr, FnDecl, Stmt};
use crate::error::{LumenError, LumenRuntimeError};
use crate::parser::parse;

/// Maximum nesting of Lumen function calls before "stack overflow" is raised.
/// Needs a large native stack: see `run_on_big_stack`.
pub const MAX_CALL_DEPTH: usize = 10_000;

type RResult<T> = Result<T, LumenRuntimeError>;

fn err<T>(line: usize, message: impl Into<String>) -> RResult<T> {
    Err(LumenRuntimeError::new(line, message))
}

// --- values ---

#[derive(Clone)]
pub enum Value {
    Nil,
    Bool(bool),
    Num(f64),
    Str(Rc<str>),
    Fn(Rc<Function>),
    Builtin(Builtin),
}

pub struct Function {
    decl: Rc<FnDecl>,
    closure: Env,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Builtin {
    Len,
    Clock,
}

impl Builtin {
    fn name(self) -> &'static str {
        match self {
            Builtin::Len => "len",
            Builtin::Clock => "clock",
        }
    }

    fn arity(self) -> usize {
        match self {
            Builtin::Len => 1,
            Builtin::Clock => 0,
        }
    }
}

impl Value {
    /// Only `false` and `nil` are falsy.
    pub fn truthy(&self) -> bool {
        !matches!(self, Value::Nil | Value::Bool(false))
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Nil => "nil",
            Value::Bool(_) => "boolean",
            Value::Num(_) => "number",
            Value::Str(_) => "string",
            Value::Fn(_) | Value::Builtin(_) => "function",
        }
    }
}

/// Lumen's `==`: values of different types are never equal; functions compare by identity.
impl PartialEq for Value {
    fn eq(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Nil, Value::Nil) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Num(a), Value::Num(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Fn(a), Value::Fn(b)) => Rc::ptr_eq(a, b),
            (Value::Builtin(a), Value::Builtin(b)) => a == b,
            _ => false,
        }
    }
}

/// How `print` renders a value.
impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Value::Nil => write!(f, "nil"),
            Value::Bool(b) => write!(f, "{b}"),
            // Integral numbers print without a trailing ".0".
            Value::Num(n) if n.fract() == 0.0 && n.abs() < 1e16 => write!(f, "{}", *n as i64),
            Value::Num(n) => write!(f, "{n}"),
            Value::Str(s) => write!(f, "{s}"),
            Value::Fn(func) => match &func.decl.name {
                Some(name) => write!(f, "<fn {name}>"),
                None => write!(f, "<fn>"),
            },
            Value::Builtin(b) => write!(f, "<builtin fn {}>", b.name()),
        }
    }
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Value::Str(s) => write!(f, "{s:?}"),
            other => write!(f, "{other}"),
        }
    }
}

// --- environments ---

type Env = Rc<Scope>;

// ponytail: a closure stored in the scope it captured forms an Rc cycle and is
// never freed. Fine for scripts; needs a tracing GC (or weak refs) for long-lived hosts.
struct Scope {
    vars: RefCell<HashMap<String, Value>>,
    parent: Option<Env>,
}

impl Scope {
    fn new(parent: Option<Env>) -> Env {
        Rc::new(Scope { vars: RefCell::new(HashMap::new()), parent })
    }

    fn define(&self, name: &str, value: Value) {
        self.vars.borrow_mut().insert(name.to_string(), value);
    }

    fn get(&self, name: &str) -> Option<Value> {
        let mut scope = self;
        loop {
            if let Some(value) = scope.vars.borrow().get(name) {
                return Some(value.clone());
            }
            scope = scope.parent.as_deref()?;
        }
    }

    /// Assign to the innermost existing binding. False if there is none.
    fn set(&self, name: &str, value: Value) -> bool {
        let mut scope = self;
        loop {
            if let Some(slot) = scope.vars.borrow_mut().get_mut(name) {
                *slot = value;
                return true;
            }
            match scope.parent.as_deref() {
                Some(parent) => scope = parent,
                None => return false,
            }
        }
    }
}

// --- interpreter ---

enum Flow {
    Next,
    Return(Value),
}

pub struct Interpreter {
    globals: Env,
    out: Box<dyn FnMut(&str)>,
    depth: usize,
}

impl Default for Interpreter {
    fn default() -> Self {
        Self::new()
    }
}

impl Interpreter {
    /// An interpreter whose `print` writes to stdout.
    pub fn new() -> Self {
        Self::with_output(|line| println!("{line}"))
    }

    /// An interpreter whose `print` hands each line to `out`.
    pub fn with_output(out: impl FnMut(&str) + 'static) -> Self {
        let globals = Scope::new(None);
        globals.define("len", Value::Builtin(Builtin::Len));
        globals.define("clock", Value::Builtin(Builtin::Clock));
        Interpreter { globals, out: Box::new(out), depth: 0 }
    }

    /// Run source in the global scope. Global state persists between calls.
    /// Returns the value of the last statement if it was an expression
    /// statement (the REPL echoes it), otherwise `None`.
    pub fn run(&mut self, source: &str) -> Result<Option<Value>, LumenError> {
        let program = parse(source)?;
        let globals = self.globals.clone();
        let mut last = None;
        for stmt in &program {
            last = match stmt {
                Stmt::Expr(expr) => Some(self.eval(expr, &globals)?),
                _ => {
                    self.exec(stmt, &globals)?;
                    None
                }
            };
        }
        Ok(last)
    }

    fn exec_all(&mut self, stmts: &[Stmt], env: &Env) -> RResult<Flow> {
        for stmt in stmts {
            if let Flow::Return(value) = self.exec(stmt, env)? {
                return Ok(Flow::Return(value));
            }
        }
        Ok(Flow::Next)
    }

    /// Run statements in a fresh child scope.
    fn exec_block(&mut self, stmts: &[Stmt], env: &Env) -> RResult<Flow> {
        self.exec_all(stmts, &Scope::new(Some(env.clone())))
    }

    fn exec(&mut self, stmt: &Stmt, env: &Env) -> RResult<Flow> {
        match stmt {
            Stmt::Expr(expr) => {
                self.eval(expr, env)?;
            }
            Stmt::Let(name, init) => {
                let value = match init {
                    Some(expr) => self.eval(expr, env)?,
                    None => Value::Nil,
                };
                env.define(name, value);
            }
            Stmt::Print(expr) => {
                let value = self.eval(expr, env)?;
                (self.out)(&value.to_string());
            }
            Stmt::If(cond, then, orelse) => {
                if self.eval(cond, env)?.truthy() {
                    return self.exec_block(then, env);
                } else if let Some(branch) = orelse {
                    return self.exec(branch, env);
                }
            }
            Stmt::While(cond, body) => {
                while self.eval(cond, env)?.truthy() {
                    if let Flow::Return(value) = self.exec_block(body, env)? {
                        return Ok(Flow::Return(value));
                    }
                }
            }
            Stmt::Return(expr) => {
                let value = match expr {
                    Some(expr) => self.eval(expr, env)?,
                    None => Value::Nil,
                };
                return Ok(Flow::Return(value));
            }
            Stmt::Block(stmts) => return self.exec_block(stmts, env),
        }
        Ok(Flow::Next)
    }

    fn eval(&mut self, expr: &Expr, env: &Env) -> RResult<Value> {
        Ok(match expr {
            Expr::Num(n) => Value::Num(*n),
            Expr::Str(s) => Value::Str(s.clone()),
            Expr::Bool(b) => Value::Bool(*b),
            Expr::Nil => Value::Nil,
            Expr::Var { name, line } => match env.get(name) {
                Some(value) => value,
                None => return err(*line, format!("undefined variable '{name}'")),
            },
            Expr::Assign { name, value, line } => {
                let value = self.eval(value, env)?;
                if !env.set(name, value.clone()) {
                    return err(*line, format!("cannot assign to undefined variable '{name}'"));
                }
                value
            }
            Expr::Neg { operand, line } => match self.eval(operand, env)? {
                Value::Num(n) => Value::Num(-n),
                other => {
                    let ty = other.type_name();
                    return err(*line, format!("operand of unary '-' must be a number, got {ty}"));
                }
            },
            Expr::Not(operand) => Value::Bool(!self.eval(operand, env)?.truthy()),
            Expr::Logic { or, left, right } => {
                let left = self.eval(left, env)?;
                if *or == left.truthy() {
                    left // short-circuit: the right side is never evaluated
                } else {
                    self.eval(right, env)?
                }
            }
            Expr::Binary { op, left, right, line } => {
                let left = self.eval(left, env)?;
                let right = self.eval(right, env)?;
                binary(op, left, right, *line)?
            }
            Expr::Call { callee, args, line } => {
                let callee = self.eval(callee, env)?;
                let mut values = Vec::with_capacity(args.len());
                for arg in args {
                    values.push(self.eval(arg, env)?);
                }
                self.call(callee, values, *line)?
            }
            Expr::Fn(decl) => {
                Value::Fn(Rc::new(Function { decl: decl.clone(), closure: env.clone() }))
            }
        })
    }

    fn call(&mut self, callee: Value, args: Vec<Value>, line: usize) -> RResult<Value> {
        match callee {
            Value::Fn(func) => {
                let params = &func.decl.params;
                if args.len() != params.len() {
                    let msg = format!(
                        "{} expects {} argument(s), got {}",
                        Value::Fn(func.clone()),
                        params.len(),
                        args.len()
                    );
                    return err(line, msg);
                }
                if self.depth >= MAX_CALL_DEPTH {
                    return err(line, "stack overflow");
                }
                // Parameters and the body's top-level `let`s share one scope,
                // whose parent is where the function was defined (lexical scoping).
                let env = Scope::new(Some(func.closure.clone()));
                for (param, arg) in params.iter().zip(args) {
                    env.define(param, arg);
                }
                self.depth += 1;
                let flow = self.exec_all(&func.decl.body, &env);
                self.depth -= 1;
                Ok(match flow? {
                    Flow::Return(value) => value,
                    Flow::Next => Value::Nil,
                })
            }
            Value::Builtin(builtin) => {
                if args.len() != builtin.arity() {
                    let msg = format!(
                        "{}() expects {} argument(s), got {}",
                        builtin.name(),
                        builtin.arity(),
                        args.len()
                    );
                    return err(line, msg);
                }
                match builtin {
                    Builtin::Len => match &args[0] {
                        Value::Str(s) => Ok(Value::Num(s.chars().count() as f64)),
                        other => {
                            let ty = other.type_name();
                            err(line, format!("len() expects a string, got {ty}"))
                        }
                    },
                    Builtin::Clock => {
                        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
                        Ok(Value::Num(now.as_secs_f64()))
                    }
                }
            }
            other => err(line, format!("can only call functions, not {}", other.type_name())),
        }
    }
}

fn binary(op: &str, a: Value, b: Value, line: usize) -> RResult<Value> {
    use Value::{Bool, Num, Str};
    Ok(match (op, &a, &b) {
        ("==", ..) => Bool(a == b),
        ("!=", ..) => Bool(a != b),
        ("+", Num(x), Num(y)) => Num(x + y),
        // `+` concatenates when either side is a string.
        ("+", Str(_), _) | ("+", _, Str(_)) => Str(format!("{a}{b}").into()),
        ("-", Num(x), Num(y)) => Num(x - y),
        ("*", Num(x), Num(y)) => Num(x * y),
        ("/", Num(_), Num(y)) if *y == 0.0 => return err(line, "division by zero"),
        ("%", Num(_), Num(y)) if *y == 0.0 => return err(line, "modulo by zero"),
        ("/", Num(x), Num(y)) => Num(x / y),
        ("%", Num(x), Num(y)) => {
            // Floored modulo: the result takes the sign of the divisor.
            let r = x % y;
            Num(if r != 0.0 && (r < 0.0) != (*y < 0.0) { r + y } else { r })
        }
        ("<" | "<=" | ">" | ">=", Num(x), Num(y)) => Bool(compare(op, x.partial_cmp(y))),
        ("<" | "<=" | ">" | ">=", Str(x), Str(y)) => Bool(compare(op, Some(x.cmp(y)))),
        _ => {
            let (ta, tb) = (a.type_name(), b.type_name());
            return err(line, format!("unsupported operands for '{op}': {ta} and {tb}"));
        }
    })
}

fn compare(op: &str, ordering: Option<Ordering>) -> bool {
    match (ordering, op) {
        (None, _) => false, // NaN
        (Some(o), "<") => o.is_lt(),
        (Some(o), "<=") => o.is_le(),
        (Some(o), ">") => o.is_gt(),
        (Some(o), _) => o.is_ge(),
    }
}

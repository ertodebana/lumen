//! Recursive-descent parser; binary operators use precedence climbing.

use std::rc::Rc;

use crate::ast::{Expr, FnDecl, Stmt};
use crate::error::LumenSyntaxError;
use crate::lexer::{tokenize, Tok, Token};

type PResult<T> = Result<T, LumenSyntaxError>;

/// Binary operator precedence, loosest (1) to tightest (7); 0 means "not a
/// binary operator". Prefix `not` sits at 3, between `and` and equality.
/// Unary `-` binds tighter than everything here.
fn precedence(op: &str) -> u8 {
    match op {
        "or" => 1,
        "and" => 2,
        "==" | "!=" => 4,
        "<" | "<=" | ">" | ">=" => 5,
        "+" | "-" => 6,
        "*" | "/" | "%" => 7,
        _ => 0,
    }
}
const NOT_OPERAND: u8 = 4;

pub fn parse(source: &str) -> PResult<Vec<Stmt>> {
    let mut parser = Parser { toks: tokenize(source)?, pos: 0, fn_depth: 0 };
    let program = parser.statements()?;
    if !parser.at_eof() {
        return Err(parser.error("expected expression")); // a stray `}`
    }
    Ok(program)
}

struct Parser {
    toks: Vec<Token>,
    pos: usize,
    fn_depth: usize,
}

impl Parser {
    // --- token helpers ---

    fn peek(&self) -> &Tok {
        &self.toks[self.pos].tok
    }

    fn line(&self) -> usize {
        self.toks[self.pos].line
    }

    fn prev_line(&self) -> usize {
        self.toks[self.pos - 1].line
    }

    fn is(&self, sym: &str) -> bool {
        matches!(self.peek(), Tok::Sym(s) if *s == sym)
    }

    fn at_eof(&self) -> bool {
        matches!(self.peek(), Tok::Eof)
    }

    fn advance(&mut self) {
        if !self.at_eof() {
            self.pos += 1;
        }
    }

    fn eat(&mut self, sym: &str) -> bool {
        let hit = self.is(sym);
        if hit {
            self.advance();
        }
        hit
    }

    fn expect(&mut self, sym: &str, what: &str) -> PResult<()> {
        if self.eat(sym) {
            Ok(())
        } else {
            Err(self.error(&format!("expected {what}")))
        }
    }

    fn ident(&mut self, what: &str) -> PResult<String> {
        match self.peek() {
            Tok::Ident(name) => {
                let name = name.clone();
                self.advance();
                Ok(name)
            }
            _ => Err(self.error(&format!("expected {what}"))),
        }
    }

    fn error(&self, msg: &str) -> LumenSyntaxError {
        let found = match self.peek() {
            Tok::Eof => "end of input".to_string(),
            Tok::Num(_) => "number".to_string(),
            Tok::Str(_) => "string".to_string(),
            Tok::Ident(s) => format!("'{s}'"),
            Tok::Sym(s) => format!("'{s}'"),
        };
        LumenSyntaxError::new(self.line(), format!("{msg}, found {found}"))
    }

    // --- statements ---

    /// Statements up to end of input, or up to (not including) a closing `}`.
    fn statements(&mut self) -> PResult<Vec<Stmt>> {
        let mut stmts = Vec::new();
        while !self.at_eof() && !self.is("}") {
            if !self.eat(";") {
                stmts.push(self.statement()?);
            }
        }
        Ok(stmts)
    }

    fn block(&mut self) -> PResult<Vec<Stmt>> {
        self.expect("{", "'{'")?;
        let stmts = self.statements()?;
        self.expect("}", "'}'")?;
        Ok(stmts)
    }

    fn end_statement(&self) -> PResult<()> {
        if self.is(";") || self.is("}") || self.at_eof() || self.line() != self.prev_line() {
            Ok(())
        } else {
            Err(self.error("expected newline or ';' after statement"))
        }
    }

    fn statement(&mut self) -> PResult<Stmt> {
        let line = self.line();
        if self.eat("let") {
            let name = self.ident("variable name")?;
            let init = if self.eat("=") { Some(self.expression()?) } else { None };
            self.end_statement()?;
            return Ok(Stmt::Let(name, init));
        }
        if self.is("fn") && matches!(self.toks[self.pos + 1].tok, Tok::Ident(_)) {
            self.advance();
            let name = self.ident("function name")?;
            let function = self.function(Some(name.clone()))?;
            return Ok(Stmt::Let(name, Some(function)));
        }
        if self.eat("print") {
            let value = self.expression()?;
            self.end_statement()?;
            return Ok(Stmt::Print(value));
        }
        if self.eat("if") {
            return self.if_statement();
        }
        if self.eat("while") {
            let cond = self.expression()?;
            return Ok(Stmt::While(cond, self.block()?));
        }
        if self.eat("return") {
            if self.fn_depth == 0 {
                return Err(LumenSyntaxError::new(line, "'return' outside of a function"));
            }
            // A return value must start on the same line as `return`.
            let bare = self.is(";") || self.is("}") || self.at_eof() || self.line() != line;
            let value = if bare { None } else { Some(self.expression()?) };
            self.end_statement()?;
            return Ok(Stmt::Return(value));
        }
        if self.is("{") {
            return Ok(Stmt::Block(self.block()?));
        }
        let value = self.expression()?;
        self.end_statement()?;
        Ok(Stmt::Expr(value))
    }

    fn if_statement(&mut self) -> PResult<Stmt> {
        let cond = self.expression()?;
        let then = self.block()?;
        let orelse = if self.eat("else") {
            let branch = if self.eat("if") { self.if_statement()? } else { Stmt::Block(self.block()?) };
            Some(Box::new(branch))
        } else {
            None
        };
        Ok(Stmt::If(cond, then, orelse))
    }

    fn function(&mut self, name: Option<String>) -> PResult<Expr> {
        self.expect("(", "'(' after 'fn'")?;
        let mut params: Vec<String> = Vec::new();
        if !self.is(")") {
            loop {
                let line = self.line();
                let param = self.ident("parameter name")?;
                if params.contains(&param) {
                    let msg = format!("duplicate parameter '{param}'");
                    return Err(LumenSyntaxError::new(line, msg));
                }
                params.push(param);
                if !self.eat(",") {
                    break;
                }
            }
        }
        self.expect(")", "')' after parameters")?;
        self.fn_depth += 1;
        let body = self.block()?;
        self.fn_depth -= 1;
        Ok(Expr::Fn(Rc::new(FnDecl { name, params, body })))
    }

    // --- expressions ---

    fn expression(&mut self) -> PResult<Expr> {
        let left = self.binary(1)?;
        if !self.is("=") {
            return Ok(left);
        }
        let line = self.line();
        self.advance();
        match left {
            // Recursing on the right-hand side makes assignment right-associative.
            Expr::Var { name, .. } => {
                Ok(Expr::Assign { name, value: Box::new(self.expression()?), line })
            }
            _ => Err(LumenSyntaxError::new(line, "invalid assignment target")),
        }
    }

    /// Precedence climbing: consume operators binding at least as tightly as `min`.
    fn binary(&mut self, min: u8) -> PResult<Expr> {
        let mut left = self.unary()?;
        loop {
            let Tok::Sym(op) = *self.peek() else { break };
            let prec = precedence(op);
            if prec < min {
                break;
            }
            let line = self.line();
            self.advance();
            // `prec + 1` makes every binary operator left-associative.
            let (l, r) = (Box::new(left), Box::new(self.binary(prec + 1)?));
            left = match op {
                "and" | "or" => Expr::Logic { or: op == "or", left: l, right: r },
                _ => Expr::Binary { op, left: l, right: r, line },
            };
        }
        Ok(left)
    }

    fn unary(&mut self) -> PResult<Expr> {
        let line = self.line();
        if self.eat("-") {
            return Ok(Expr::Neg { operand: Box::new(self.unary()?), line });
        }
        if self.eat("not") {
            return Ok(Expr::Not(Box::new(self.binary(NOT_OPERAND)?)));
        }
        self.call()
    }

    fn call(&mut self) -> PResult<Expr> {
        let mut expr = self.primary()?;
        // `(` only continues a call on the same line, so a statement may start with `(`.
        while self.is("(") && self.line() == self.prev_line() {
            let line = self.line();
            self.advance();
            let mut args = Vec::new();
            if !self.is(")") {
                loop {
                    args.push(self.expression()?);
                    if !self.eat(",") {
                        break;
                    }
                }
            }
            self.expect(")", "')' after arguments")?;
            expr = Expr::Call { callee: Box::new(expr), args, line };
        }
        Ok(expr)
    }

    fn primary(&mut self) -> PResult<Expr> {
        let line = self.line();
        let expr = match self.peek().clone() {
            Tok::Num(n) => Expr::Num(n),
            Tok::Str(s) => Expr::Str(s.into()),
            Tok::Ident(name) => Expr::Var { name, line },
            Tok::Sym("true") => Expr::Bool(true),
            Tok::Sym("false") => Expr::Bool(false),
            Tok::Sym("nil") => Expr::Nil,
            Tok::Sym("(") => {
                self.advance();
                let inner = self.expression()?;
                self.expect(")", "')'")?;
                return Ok(inner);
            }
            Tok::Sym("fn") => {
                self.advance();
                return self.function(None);
            }
            _ => return Err(self.error("expected expression")),
        };
        self.advance();
        Ok(expr)
    }
}

use std::cell::RefCell;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::rc::Rc;

use lumen::ast::{Expr, Stmt};
use lumen::{parse, tokenize, Interpreter, LumenError, Session, Tok, Value};

// --- helpers ---

/// Run a program; return everything it printed and how it ended.
fn run(src: &str) -> (Vec<String>, Result<(), LumenError>) {
    lumen::run_on_big_stack(|| {
        let out = Rc::new(RefCell::new(Vec::new()));
        let sink = out.clone();
        let mut interp = Interpreter::with_output(move |line| sink.borrow_mut().push(line.into()));
        let result = interp.run(src).map(|_| ());
        let printed = out.borrow().clone();
        (printed, result)
    })
}

fn ok(src: &str) -> Vec<String> {
    let (out, result) = run(src);
    if let Err(e) = result {
        panic!("program failed: {e}\n{src}");
    }
    out
}

fn fails(src: &str) -> LumenError {
    run(src).1.expect_err(&format!("program should have failed:\n{src}"))
}

fn assert_syntax_error(src: &str, line: usize, text: &str) {
    let e = fails(src);
    assert!(matches!(e, LumenError::Syntax(_)), "{src:?} gave {e}");
    assert_eq!(e.line(), line, "{src:?} gave {e}");
    assert!(e.message().contains(text), "{src:?} gave {e}");
    assert!(e.to_string().starts_with(&format!("[line {line}] SyntaxError: ")));
}

fn assert_runtime_error(src: &str, line: usize, text: &str) {
    let e = fails(src);
    assert!(matches!(e, LumenError::Runtime(_)), "{src:?} gave {e}");
    assert_eq!(e.line(), line, "{src:?} gave {e}");
    assert!(e.message().contains(text), "{src:?} gave {e}");
    assert!(e.to_string().starts_with(&format!("[line {line}] RuntimeError: ")));
}

fn assert_prints(cases: &[(&str, &str)]) {
    for (expr, expected) in cases {
        assert_eq!(ok(&format!("print {expr}")), [*expected], "print {expr}");
    }
}

fn kinds(src: &str) -> Vec<Tok> {
    tokenize(src).unwrap().into_iter().map(|t| t.tok).collect()
}

fn sym(s: &'static str) -> Tok {
    Tok::Sym(s)
}

fn ident(s: &str) -> Tok {
    Tok::Ident(s.into())
}

fn cli(args: &[&str], stdin: Option<&str>) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_lumen"))
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(input) = stdin {
        child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    }
    child.wait_with_output().unwrap()
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn temp_script(name: &str, src: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("lumen-test-{}-{name}.lm", std::process::id()));
    std::fs::write(&path, src).unwrap();
    path
}

// --- lexer ---

#[test]
fn lex_numbers_are_floats() {
    assert_eq!(kinds("42 3.14"), [Tok::Num(42.0), Tok::Num(3.14), Tok::Eof]);
}

#[test]
fn lex_string_escapes() {
    assert_eq!(kinds(r#""a\nb\t\"q\"\\""#)[0], Tok::Str("a\nb\t\"q\"\\".into()));
}

#[test]
fn lex_keywords_vs_identifiers() {
    assert_eq!(
        kinds("let letter fn fn_1 nil _x"),
        [sym("let"), ident("letter"), sym("fn"), ident("fn_1"), sym("nil"), ident("_x"), Tok::Eof]
    );
}

#[test]
fn lex_all_keywords() {
    let src = "let fn return if else while print true false nil and or not";
    let expected: Vec<Tok> = src.split(' ').map(|w| kinds(w)[0].clone()).collect();
    assert_eq!(kinds(src)[..13], expected[..]);
    assert!(expected.iter().all(|t| matches!(t, Tok::Sym(_))));
}

#[test]
fn lex_operators() {
    let src = "+ - * / % == != < <= > >= = ( ) { } , ;";
    let texts: Vec<String> = kinds(src)
        .into_iter()
        .filter_map(|t| if let Tok::Sym(s) = t { Some(s.to_string()) } else { None })
        .collect();
    assert_eq!(texts, src.split(' ').collect::<Vec<_>>());
}

#[test]
fn lex_comments_and_line_tracking() {
    let toks = tokenize("a # ignored == \"\nb\n\n  c").unwrap();
    let got: Vec<(Tok, usize)> = toks.into_iter().map(|t| (t.tok, t.line)).collect();
    assert_eq!(got, [(ident("a"), 1), (ident("b"), 2), (ident("c"), 4), (Tok::Eof, 4)]);
}

#[test]
fn lex_errors() {
    for (src, line, text) in [
        ("let s = \"abc", 1, "unterminated string"),
        ("\n\"bad \\q\"", 2, "invalid escape"),
        ("let a = 1\nlet b = @", 2, "unexpected character '@'"),
        ("1 ! 2", 1, "unexpected character '!'"),
    ] {
        let e = tokenize(src).unwrap_err();
        assert_eq!(e.line, line, "{src:?}");
        assert!(e.message.contains(text), "{src:?} gave {e}");
    }
}

// --- precedence and operators ---

#[test]
fn parse_tree_shape() {
    let num = |n| Box::new(Expr::Num(n));
    let product = Expr::Binary { op: "*", left: num(2.0), right: num(3.0), line: 1 };
    let sum = Expr::Binary { op: "+", left: num(1.0), right: Box::new(product), line: 1 };
    assert_eq!(parse("1 + 2 * 3").unwrap(), [Stmt::Expr(sum)]);
}

#[test]
fn precedence_arithmetic() {
    assert_prints(&[
        ("1 + 2 * 3", "7"),
        ("(1 + 2) * 3", "9"),
        ("2 * 3 % 4", "2"),
        ("-2 * 3", "-6"),
        ("2 - -3", "5"),
    ]);
}

#[test]
fn binary_operators_are_left_associative() {
    assert_prints(&[("10 - 4 - 3", "3"), ("100 / 10 / 5", "2")]);
}

#[test]
fn precedence_comparison_and_equality() {
    assert_prints(&[
        ("1 + 2 < 4", "true"),     // arithmetic binds tighter than comparison
        ("1 < 2 == true", "true"), // comparison binds tighter than equality
        ("1 == 1 and 2 != 3", "true"),
    ]);
}

#[test]
fn precedence_logical() {
    assert_prints(&[
        ("not 1 == 2", "true"),              // not binds looser than comparison
        ("not true or true", "true"),        // ...but tighter than and/or
        ("true or false and false", "true"), // and binds tighter than or
    ]);
}

#[test]
fn assignment_is_a_right_associative_expression() {
    assert_eq!(ok("let a = 0\nlet b = 0\na = b = 5\nprint a + b"), ["10"]);
}

#[test]
fn numbers_print_without_trailing_zero() {
    assert_prints(&[
        ("3", "3"),
        ("6 / 2", "3"),
        ("7 / 2", "3.5"),
        ("2.50", "2.5"),
        ("-0.25", "-0.25"),
        ("0.1 + 0.2", "0.30000000000000004"),
    ]);
}

#[test]
fn modulo_takes_the_sign_of_the_divisor() {
    assert_prints(&[("7 % 3", "1"), ("-7 % 3", "2"), ("7 % -3", "-2")]);
}

#[test]
fn strings_concatenate_and_compare() {
    assert_prints(&[
        ("\"a\" + \"b\"", "ab"),
        ("\"n = \" + 4", "n = 4"),
        ("1.5 + \" is \" + true + \" or \" + nil", "1.5 is true or nil"),
        ("\"abc\" < \"abd\"", "true"),
    ]);
}

#[test]
fn equality_never_coerces() {
    assert_prints(&[("1 == true", "false"), ("nil == false", "false"), ("\"1\" == 1", "false")]);
}

// --- scoping ---

#[test]
fn block_shadowing() {
    assert_eq!(ok("let x = 1\n{\n let x = 2\n print x\n}\nprint x"), ["2", "1"]);
}

#[test]
fn assignment_reaches_outer_scope() {
    assert_eq!(ok("let x = 1\n{ x = 2 }\nprint x"), ["2"]);
}

#[test]
fn block_local_is_not_visible_outside() {
    assert_runtime_error("{ let y = 1 }\nprint y", 2, "undefined variable 'y'");
}

#[test]
fn scoping_is_lexical_not_dynamic() {
    let src = r#"
        let x = "global"
        fn show() { return x }
        fn test() {
            let x = "local"
            return show()
        }
        print test()
    "#;
    assert_eq!(ok(src), ["global"]);
}

#[test]
fn let_without_initializer_is_nil() {
    assert_eq!(ok("let x\nprint x"), ["nil"]);
}

// --- closures and first-class functions ---

const COUNTER: &str = "
fn make_counter() {
    let count = 0
    return fn() {
        count = count + 1
        return count
    }
}
";

#[test]
fn closure_counter_keeps_state() {
    assert_eq!(ok(&format!("{COUNTER}let c = make_counter()\nc()\nc()\nprint c()")), ["3"]);
}

#[test]
fn closures_are_independent() {
    let src = format!(
        "{COUNTER}let a = make_counter()\nlet b = make_counter()\na()\na()\nprint a()\nprint b()"
    );
    assert_eq!(ok(&src), ["3", "1"]);
}

#[test]
fn closure_captures_variable_not_value() {
    assert_eq!(ok("let x = 1\nlet get = fn() { return x }\nx = 2\nprint get()"), ["2"]);
}

#[test]
fn higher_order_functions() {
    let src = "
        fn compose(f, g) { return fn(x) { return f(g(x)) } }
        fn inc(x) { return x + 1 }
        let double = fn(x) { return x * 2 }
        print compose(double, inc)(5)
        print fn(x) { return x * x }(4)
    ";
    assert_eq!(ok(src), ["12", "16"]);
}

#[test]
fn function_values_print_and_compare() {
    let src = "fn f() {}\nlet g = f\nprint f\nprint fn() {}\nprint len\nprint f == g\nprint f()";
    assert_eq!(ok(src), ["<fn f>", "<fn>", "<builtin fn len>", "true", "nil"]);
}

// --- recursion ---

#[test]
fn recursion_factorial_and_fib() {
    let src = "
        fn fact(n) { if n <= 1 { return 1 } return n * fact(n - 1) }
        fn fib(n) { if n < 2 { return n } return fib(n - 1) + fib(n - 2) }
        print fact(10)
        print fib(15)
    ";
    assert_eq!(ok(src), ["3628800", "610"]);
}

#[test]
fn mutual_recursion() {
    let src = "
        fn is_even(n) { if n == 0 { return true } return is_odd(n - 1) }
        fn is_odd(n) { if n == 0 { return false } return is_even(n - 1) }
        print is_even(10)
        print is_even(7)
    ";
    assert_eq!(ok(src), ["true", "false"]);
}

#[test]
fn recursive_anonymous_function() {
    let src = "let f = fn(n) { if n == 0 { return 0 } return n + f(n - 1) }\nprint f(100)";
    assert_eq!(ok(src), ["5050"]);
}

#[test]
fn deep_recursion_within_the_limit() {
    let src = "fn depth(n) { if n == 0 { return 0 } return 1 + depth(n - 1) }\nprint depth(9000)";
    assert_eq!(ok(src), ["9000"]);
}

// --- control flow ---

#[test]
fn if_else_chain() {
    let src = r#"
        fn sign(n) {
            if n < 0 { return "neg" } else if n == 0 { return "zero" } else { return "pos" }
        }
        print sign(-5)
        print sign(0)
        print sign(5)
    "#;
    assert_eq!(ok(src), ["neg", "zero", "pos"]);
}

#[test]
fn while_loop() {
    let src = "let i = 0\nlet sum = 0\nwhile i < 5 { i = i + 1\n sum = sum + i }\nprint sum";
    assert_eq!(ok(src), ["15"]);
}

#[test]
fn return_exits_nested_loops() {
    let src = "
        fn find() {
            let i = 0
            while true {
                while true {
                    if i == 3 { return i }
                    i = i + 1
                }
            }
        }
        print find()
    ";
    assert_eq!(ok(src), ["3"]);
}

#[test]
fn bare_return_gives_nil() {
    assert_eq!(ok("fn f() {\n return\n}\nprint f()"), ["nil"]);
}

#[test]
fn only_false_and_nil_are_falsy() {
    let src = r#"
        if 0 { print "zero" }
        if "" { print "empty" }
        if nil { print "nil" }
        if false { print "false" }
    "#;
    assert_eq!(ok(src), ["zero", "empty"]);
}

#[test]
fn and_or_short_circuit() {
    let src = r#"
        fn boom() { print "evaluated"
            return true }
        print false and boom()
        print true or boom()
        print nil or "default"
        print 1 and 2
    "#;
    assert_eq!(ok(src), ["false", "true", "default", "2"]);
}

// --- builtins ---

#[test]
fn builtin_len() {
    assert_eq!(ok("print len(\"hello\")\nprint len(\"\")\nprint len(\"héllo\")"), ["5", "0", "5"]);
}

#[test]
fn builtin_clock() {
    assert_eq!(ok("let t = clock()\nprint t > 0\nprint clock() >= t"), ["true", "true"]);
}

// --- syntax errors ---

#[test]
fn syntax_error_missing_expression() {
    assert_syntax_error("let x =", 1, "expected expression, found end of input");
    assert_syntax_error("f(1,", 1, "expected expression");
    assert_syntax_error("let x = 1\nelse { }", 2, "expected expression, found 'else'");
}

#[test]
fn syntax_error_missing_names() {
    assert_syntax_error("let = 5", 1, "expected variable name");
    assert_syntax_error("let f = fn(1) {}", 1, "expected parameter name");
}

#[test]
fn syntax_error_unbalanced_brackets() {
    assert_syntax_error("print 1\nif true {\n print 2", 3, "expected '}'");
    assert_syntax_error("print (1 + 2", 1, "expected ')'");
    assert_syntax_error("print 1\n}", 2, "expected expression, found '}'");
    assert_syntax_error("if true print 1", 1, "expected '{'");
}

#[test]
fn syntax_error_invalid_assignment_target() {
    assert_syntax_error("let a = 1\n1 + a = 2", 2, "invalid assignment target");
}

#[test]
fn syntax_error_return_outside_function() {
    assert_syntax_error("\n\nreturn 1", 3, "'return' outside of a function");
}

#[test]
fn syntax_error_two_statements_on_one_line() {
    assert_syntax_error("print 1 print 2", 1, "expected newline or ';'");
    assert_eq!(ok("print 1; print 2"), ["1", "2"]);
}

#[test]
fn syntax_error_duplicate_parameter() {
    assert_syntax_error("fn f(a, a) {}", 1, "duplicate parameter 'a'");
}

#[test]
fn lexer_errors_surface_as_syntax_errors() {
    assert_syntax_error("print 1\nprint \"oops", 2, "unterminated string");
}

// --- runtime errors ---

#[test]
fn runtime_error_undefined_variable() {
    assert_runtime_error("let a = 1\nprint b", 2, "undefined variable 'b'");
    assert_runtime_error("\nx = 1", 2, "cannot assign to undefined variable 'x'");
}

#[test]
fn runtime_error_operand_types() {
    assert_runtime_error("print 1 - \"a\"", 1, "unsupported operands for '-': number and string");
    assert_runtime_error("print nil + 1", 1, "unsupported operands for '+': nil and number");
    assert_runtime_error("print 1 < \"a\"", 1, "unsupported operands for '<'");
    assert_runtime_error("print true * 2", 1, "unsupported operands for '*': boolean and number");
    assert_runtime_error("print -\"a\"", 1, "unary '-' must be a number, got string");
}

#[test]
fn runtime_error_division_and_modulo_by_zero() {
    assert_runtime_error("let z = 0\n\nprint 1 / z", 3, "division by zero");
    assert_runtime_error("print 1 % 0", 1, "modulo by zero");
}

#[test]
fn runtime_error_calling_a_non_function() {
    assert_runtime_error("let s = \"x\"\ns()", 2, "can only call functions, not string");
}

#[test]
fn runtime_error_wrong_argument_count() {
    assert_runtime_error("fn f(a, b) {}\nf(1)", 2, "<fn f> expects 2 argument(s), got 1");
    assert_runtime_error("len(\"a\", \"b\")", 1, "len() expects 1 argument(s), got 2");
}

#[test]
fn runtime_error_bad_builtin_argument() {
    assert_runtime_error("print len(5)", 1, "len() expects a string, got number");
}

#[test]
fn runtime_error_line_inside_function_body() {
    assert_runtime_error("fn f() {\n let a = 1\n return a + nil\n}\nf()", 3, "unsupported");
}

#[test]
fn runtime_error_stack_overflow() {
    assert_runtime_error("fn r(n) { return r(n + 1) }\n\nr(0)", 1, "stack overflow");
}

#[test]
fn output_before_a_runtime_error_is_kept() {
    let (out, result) = run("print 1\nprint nope\nprint 2");
    assert_eq!(out, ["1"]);
    assert!(result.is_err());
}

// --- REPL session ---

#[test]
fn repl_session_handles_multiline_blocks_and_keeps_state() {
    let mut s = Session::new(Interpreter::with_output(|_| {}));
    assert_eq!(s.feed("let x = 2"), Some(Ok(None)));
    assert_eq!(s.feed("fn triple(n) {"), None); // waiting for the closing brace
    assert!(s.pending());
    assert_eq!(s.feed("    return n * 3"), None);
    assert_eq!(s.feed("}"), Some(Ok(None)));
    assert!(!s.pending());
    assert_eq!(s.feed("triple(x)"), Some(Ok(Some(Value::Num(6.0))))); // echoed by the REPL
    let failure = s.feed("print nope").unwrap().unwrap_err();
    assert_eq!(failure.message(), "undefined variable 'nope'");
    assert_eq!(s.feed("x + 1"), Some(Ok(Some(Value::Num(3.0))))); // session survives errors
}

#[test]
fn repl_session_reset_discards_partial_input() {
    let mut s = Session::new(Interpreter::with_output(|_| {}));
    assert_eq!(s.feed("fn f() {"), None);
    s.reset();
    assert_eq!(s.feed("1 + 1"), Some(Ok(Some(Value::Num(2.0)))));
}

// --- CLI ---

#[test]
fn cli_runs_a_file() {
    let path = temp_script("ok", "#!/usr/bin/env lumen\nprint \"hi\"\nprint 1 + 2\n");
    let o = cli(&[path.to_str().unwrap()], None);
    std::fs::remove_file(path).unwrap();
    assert!(o.status.success());
    assert_eq!((stdout(&o).as_str(), stderr(&o).as_str()), ("hi\n3\n", ""));
}

#[test]
fn cli_errors_exit_nonzero_with_line_number() {
    for (name, src, text) in [
        ("rt", "print 1\nprint nope\n", "[line 2] RuntimeError: undefined variable 'nope'"),
        ("syn", "print 1\nlet = 2\n", "[line 2] SyntaxError: expected variable name"),
    ] {
        let path = temp_script(name, src);
        let o = cli(&[path.to_str().unwrap()], None);
        std::fs::remove_file(path).unwrap();
        assert_eq!(o.status.code(), Some(1));
        assert!(stderr(&o).contains(text), "{}", stderr(&o));
    }
}

#[test]
fn cli_missing_file_exits_nonzero() {
    let o = cli(&["definitely-not-here.lm"], None);
    assert_eq!(o.status.code(), Some(1));
    assert!(stderr(&o).contains("definitely-not-here.lm"));
}

#[test]
fn cli_dash_c_runs_a_string() {
    let o = cli(&["-c", "print \"hi \" + len(\"abc\")"], None);
    assert_eq!((o.status.code(), stdout(&o).as_str()), (Some(0), "hi 3\n"));
    let o = cli(&["-c", "print nope"], None);
    assert_eq!(o.status.code(), Some(1));
    assert!(stderr(&o).contains("[line 1] RuntimeError"));
}

#[test]
fn cli_piped_stdin_runs_as_a_script() {
    let o = cli(&[], Some("let a = 20\nif a > 10 {\nprint a + 1\n}\n"));
    assert_eq!((o.status.code(), stdout(&o).as_str()), (Some(0), "21\n"));
    assert_eq!(stdout(&cli(&["-"], Some("print 1"))), "1\n");
}

#[test]
fn cli_version_help_and_usage() {
    assert_eq!(stdout(&cli(&["--version"], None)), format!("Lumen {}\n", lumen::VERSION));
    assert!(stdout(&cli(&["--help"], None)).starts_with("usage: lumen"));
    assert_eq!(cli(&["a.lm", "b.lm"], None).status.code(), Some(2));
    assert_eq!(cli(&["--bogus"], None).status.code(), Some(2));
}

#[test]
fn example_matches_documented_output() {
    let o = cli(&["examples/closures.lm"], None);
    assert!(o.status.success());
    let expected = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/closures.expected.txt");
    assert_eq!(stdout(&o), std::fs::read_to_string(expected).unwrap());
}

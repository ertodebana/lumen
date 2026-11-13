# Lumen

A small dynamically typed programming language with a tree-walking interpreter,
written in Rust. It builds to a single native `lumen` executable.

- Website: <https://lumen-lang.vercel.app>
- Documentation: <https://ertodebana.github.io/lumen/>
- Handbook (PDF): <https://ertodebana.github.io/lumen/Lumen-Handbook.pdf>

```lumen
fn make_counter() {
    let count = 0
    return fn() {
        count = count + 1
        return count
    }
}

let next = make_counter()
next()
print "count: " + next()    # count: 2
```

## Handbook

`docs/Lumen-Handbook.pdf` is a 45-page guide and reference. Rebuild it with
`python3 handbook/build.py` (needs `lumen` on your `PATH`, Chrome and `pdftotext`);
the build runs every example through `lumen` and prints the real output.
The same build writes the documentation website into `docs/`, which GitHub Pages serves.

## Install

```sh
cargo install --path .      # puts `lumen` in ~/.cargo/bin
```

Or build without installing: `cargo build --release`, then run `target/release/lumen`.

## Running

```sh
lumen                       # interactive REPL
lumen examples/closures.lm  # run a script
lumen -c 'print 6 * 7'      # run a program passed as a string
echo 'print 1' | lumen      # run a program from stdin (also: lumen -)
lumen --version
lumen --help
```

Scripts can be made directly executable, because `#` starts a comment:

```sh
$ head -1 examples/closures.lm
#!/usr/bin/env lumen
$ chmod +x examples/closures.lm && ./examples/closures.lm
```

Errors go to stderr as `[line N] SyntaxError: ...` or `[line N] RuntimeError: ...`.
Exit codes: 0 on success, 1 on a Lumen error or unreadable file, 2 on bad usage.

### The REPL

```text
$ lumen
Lumen 0.2.0 (linux x86_64)
Ctrl-D to exit.
> fn square(n) {
...     return n * n
... }
> square(12)
144
> print nope
[line 1] RuntimeError: undefined variable 'nope'
>
```

- It keeps reading (prompt `...`) while `{` or `(` are unclosed, so blocks can span lines.
- The value of a bare expression is echoed unless it is `nil`.
- Variables and functions persist between entries; an error does not end the session.
- Arrow keys and line editing work, and history is saved to `~/.lumen_history`.
- Ctrl-C discards a half-typed entry; Ctrl-D exits.

## Testing

```sh
cargo test
```

65 tests in `tests/lumen.rs` cover the lexer, precedence, scoping, closures,
recursion, control flow, builtins, every syntax and runtime error path, the REPL
session and the command line. One test runs the example and compares it with its
documented output, and the Rust snippet in this README is compiled and run as a
doc test.

## Language overview

### Values

| Type     | Examples                    | Notes                                         |
|----------|-----------------------------|-----------------------------------------------|
| number   | `42`, `3.14`                | All numbers are 64-bit floats. `6 / 2` prints `3`. |
| string   | `"hi\n"`                    | Escapes: `\n` `\t` `\"` `\\`. May span lines. |
| boolean  | `true`, `false`             |                                               |
| nil      | `nil`                       |                                               |
| function | `fn(x) { return x }`, `len` | First-class; closes over its environment.     |

Only `false` and `nil` are falsy. `0` and `""` are truthy.

### Statements

```lumen
let x = 10              # declare (initializer optional, defaults to nil)
x = x + 1               # assignment is an expression; the variable must exist
print x                 # print one value followed by a newline

if x > 10 {
    print "big"
} else if x == 10 {
    print "ten"
} else {
    print "small"
}

while x > 0 { x = x - 1 }

fn add(a, b) {          # sugar for: let add = fn(a, b) { ... }
    return a + b        # bare `return` (or falling off the end) returns nil
}

{ let scoped = 1 }      # a block is a new scope
```

`#` starts a comment that runs to the end of the line.

### Operators, loosest to tightest

| Operators            | Notes                                                    |
|----------------------|----------------------------------------------------------|
| `=`                  | right-associative                                        |
| `or`                 | short-circuits; yields the deciding operand              |
| `and`                | short-circuits; yields the deciding operand              |
| `not`                | prefix; `not a == b` means `not (a == b)`                |
| `==` `!=`            | any types; values of different types are never equal     |
| `<` `<=` `>` `>=`    | two numbers or two strings                               |
| `+` `-`              | `+` also concatenates when either side is a string       |
| `*` `/` `%`          |                                                          |
| unary `-`            |                                                          |
| call `f(x)`          |                                                          |

All binary operators are left-associative. Parentheses group.

### Functions and closures

Functions are values. They capture the scope they were created in (the variable,
not a snapshot of its value), and can be anonymous, passed around, returned, and
called immediately:

```lumen
fn compose(f, g) { return fn(x) { return f(g(x)) } }
let inc = fn(x) { return x + 1 }
print compose(inc, inc)(5)          # 7
print fn(x) { return x * x }(4)     # 16
```

### Builtins

- `len(s)`: number of characters in a string.
- `clock()`: seconds since the Unix epoch, as a number.

### Errors

Both error types carry a `line` and a `message`:

- `LumenSyntaxError`: raised by the lexer and parser.
- `LumenRuntimeError`: undefined variable, bad operand types, division or modulo
  by zero, calling a non-function, wrong argument count, bad builtin argument,
  stack overflow.

`LumenError` is an enum over the two, returned by `Interpreter::run`.

```text
$ lumen -c 'let total = 10
print total / (total - 10)'
[line 2] RuntimeError: division by zero
```

## Example

`examples/closures.lm` exercises closures (counters, `adder`, `compose`) and
recursion (`fact`, `fib`, a recursive `repeat` that builds closures, and Towers of
Hanoi driven by a callback). Its expected output is stored in
`examples/closures.expected.txt`:

```text
counter a: 3
counter b: 1
add5(10) = 15
double(add5(1)) = 12
fact(10) = 3628800
fib: 0 1 1 2 3 5 8 13 21 34
double x8 of 1 = 256
move A -> B
move A -> C
move B -> C
hanoi(10) takes 1023 moves
```

## Embedding in Rust

```rust
use lumen::{Interpreter, LumenError};

let mut interp = Interpreter::new(); // `print` goes to stdout
interp.run("let greeting = \"2 + 2 = \"").unwrap();
interp.run("print greeting + (2 + 2)").unwrap(); // globals persist between runs

match interp.run("print 1 / 0") {
    Err(LumenError::Runtime(e)) => assert_eq!((e.line, e.message.as_str()), (1, "division by zero")),
    other => panic!("expected a runtime error, got {other:?}"),
}
```

`Interpreter::with_output(|line| ...)` captures printed lines instead. For deeply
recursive Lumen programs, wrap the work in `lumen::run_on_big_stack(|| ...)`, as
the `lumen` binary does.

## Layout

```text
src/error.rs          LumenSyntaxError, LumenRuntimeError, LumenError
src/lexer.rs          tokenize(source) -> Vec<Token>
src/ast.rs            Expr and Stmt
src/parser.rs         parse(source) -> Vec<Stmt>
src/interpreter.rs    Interpreter, Value, scopes, builtins
src/lib.rs            public API, Session (REPL state), run_on_big_stack
src/main.rs           the `lumen` command: file runner, -c, stdin, REPL
tests/lumen.rs        test suite
examples/             closures.lm and its expected output
```

The only dependency is `rustyline`, for line editing and history in the REPL.

## Design decisions

The spec left these open; this is what was chosen.

1. **No statement terminator.** Statements end at a newline. `;` is accepted as
   an optional separator, and two statements on one line without it is a syntax
   error. An expression continues onto the next line if that line starts with a
   binary operator.
2. **A call's `(` must be on the same line as the callee**, and a `return` value
   must start on the same line as `return`. This keeps newline-separated
   statements unambiguous.
3. **Punctuation beyond the spec's operator list**: `( ) { } , ;` are tokens,
   since calls, blocks and parameter lists need them.
4. **Braces are mandatory** for `if`, `while` and function bodies; conditions take
   no parentheses.
5. **`print` is a statement**, not a function. `print(x)` works because the
   parentheses are just grouping.
6. **`not` has Python-style precedence**: looser than comparison, tighter than
   `and`. Unary `-` binds tightest.
7. **`and` / `or` return an operand**, not a coerced boolean (`nil or "x"` is `"x"`).
8. **Truthiness**: only `false` and `nil` are falsy.
9. **`+` stringifies when either operand is a string**, so `"n = " + 4` works.
   This replaces a `str()` builtin. Every other arithmetic operator requires numbers.
10. **Equality never errors and never coerces**: `1 == true` and `"1" == 1` are
    false. Functions compare by identity.
11. **`%` is floored modulo**: the result takes the sign of the divisor (`-7 % 3` is `2`).
12. **Division and modulo by zero are runtime errors** rather than infinity or NaN.
13. **`fn name() {}` is sugar for `let name = fn() {}`.** Parameters and the body's
    top-level `let`s share one scope. `let` may redeclare a name in the same scope.
14. **Assigning to an undeclared variable is a runtime error**; there are no
    implicit globals.
15. **`return` outside a function and duplicate parameter names are syntax errors.**
16. **Strict arity**: calling with the wrong number of arguments is a runtime error.
17. **Number printing**: integral values print without a decimal point; others
    print the shortest digits that round-trip, so `0.1 + 0.2` prints
    `0.30000000000000004`.
18. **Runtime error lines** are the line of the operator, the call's `(`, or the
    variable name involved.
19. **Piped stdin runs as a script**, not as a REPL session, like `python` does.
20. **Call depth is capped at 10,000** nested calls; beyond that the program gets
    `RuntimeError: stack overflow` instead of crashing. The binary runs the
    interpreter on a 256 MiB stack to make room for that.
21. **Implementation**: the AST is two enums, values use `Rc`, scopes are a chain
    of hash maps, and `return` is propagated as a value (`Flow::Return`) rather
    than by unwinding.

## Known gaps

- A line that starts with `-` continues the previous line's expression
  (`let a = 1` then `-a` parses as `1 - a`). Use `;` to separate them.
- In the REPL, `else` must be on the same line as the closing `}` of the `if`
  block, since a balanced entry is executed immediately. Line numbers in REPL
  errors are relative to the current entry.
- Ctrl-C while a program is running (for example an infinite loop) terminates
  `lumen`, including the REPL.
- Memory: there is no garbage collector. A closure stored in the scope it
  captured forms a reference cycle and is not freed until the process exits.
- Embedders who call `Interpreter::run` on a default-size thread stack without
  `run_on_big_stack` can exhaust the native stack before the 10,000-call cap.
  Extremely deeply nested expressions have the same limit. There is no tail-call
  optimisation.
- No `break` / `continue`, no lists or maps, no string indexing, no `for` loop.
- Number literals have no exponent form and need a digit on both sides of the
  point (`.5` and `5.` are not numbers).
- Syntax errors stop at the first error; there is no recovery.

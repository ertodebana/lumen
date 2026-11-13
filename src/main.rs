use std::io::{IsTerminal, Read};
use std::path::PathBuf;
use std::process::ExitCode;

use lumen::{Interpreter, Session, Value, VERSION};
use rustyline::error::ReadlineError;

const USAGE: &str = "\
usage: lumen [file | -c CODE | -]

  lumen               start the interactive REPL
  lumen file.lm       run a script
  lumen -c CODE       run a program passed as a string
  lumen -             run a program read from stdin (also when stdin is piped)
  lumen -V, --version print the version
  lumen -h, --help    print this help";

fn main() -> ExitCode {
    // The interpreter recurses on the native stack, so give it a big one.
    lumen::run_on_big_stack(real_main)
}

fn real_main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let code = match args.as_slice() {
        ["-h" | "--help"] => {
            println!("{USAGE}");
            0
        }
        ["-V" | "--version"] => {
            println!("Lumen {VERSION}");
            0
        }
        ["-c", code] => run_source(code),
        ["-"] => run_stdin(),
        [] if !std::io::stdin().is_terminal() => run_stdin(),
        [] => repl(),
        [file] if !file.starts_with('-') => match std::fs::read_to_string(file) {
            Ok(source) => run_source(&source),
            Err(e) => {
                eprintln!("lumen: {file}: {e}");
                1
            }
        },
        _ => {
            eprintln!("{USAGE}");
            2
        }
    };
    ExitCode::from(code)
}

fn run_source(source: &str) -> u8 {
    match Interpreter::new().run(source) {
        Ok(_) => 0,
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

fn run_stdin() -> u8 {
    let mut source = String::new();
    match std::io::stdin().read_to_string(&mut source) {
        Ok(_) => run_source(&source),
        Err(e) => {
            eprintln!("lumen: stdin: {e}");
            1
        }
    }
}

fn repl() -> u8 {
    let mut editor = match rustyline::DefaultEditor::new() {
        Ok(editor) => editor,
        Err(e) => {
            eprintln!("lumen: {e}");
            return 1;
        }
    };
    let history = std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".lumen_history"));
    if let Some(path) = &history {
        let _ = editor.load_history(path); // absent on first run
    }

    println!("Lumen {VERSION} ({} {})", std::env::consts::OS, std::env::consts::ARCH);
    println!("Ctrl-D to exit.");

    let mut session = Session::default();
    loop {
        match editor.readline(if session.pending() { "... " } else { "> " }) {
            Ok(line) => {
                let _ = editor.add_history_entry(line.as_str());
                match session.feed(&line) {
                    None => {} // brackets still open: keep reading
                    Some(Ok(Some(value))) if value != Value::Nil => println!("{value}"),
                    Some(Ok(_)) => {}
                    Some(Err(e)) => eprintln!("{e}"),
                }
            }
            Err(ReadlineError::Interrupted) => session.reset(),
            Err(_) => break, // Ctrl-D
        }
    }
    if let Some(path) = &history {
        let _ = editor.save_history(path);
    }
    0
}

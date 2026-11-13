use crate::error::LumenSyntaxError;

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Num(f64),
    Str(String),
    Ident(String),
    /// A keyword, operator or punctuation mark, by its source text.
    Sym(&'static str),
    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub tok: Tok,
    pub line: usize,
}

const KEYWORDS: [&str; 13] = [
    "let", "fn", "return", "if", "else", "while", "print", "true", "false", "nil", "and", "or",
    "not",
];

// Two-character symbols first so that `==` is not lexed as `=` `=`.
const SYMBOLS: [&str; 18] = [
    "==", "!=", "<=", ">=", "+", "-", "*", "/", "%", "<", ">", "=", "(", ")", "{", "}", ",", ";",
];

pub fn tokenize(src: &str) -> Result<Vec<Token>, LumenSyntaxError> {
    let chars: Vec<char> = src.chars().collect();
    let n = chars.len();
    let (mut i, mut line) = (0, 1);
    let mut out = Vec::new();

    while i < n {
        let c = chars[i];
        match c {
            '\n' => {
                line += 1;
                i += 1;
            }
            ' ' | '\t' | '\r' => i += 1,
            '#' => {
                while i < n && chars[i] != '\n' {
                    i += 1;
                }
            }
            '0'..='9' => {
                let start = i;
                while i < n && chars[i].is_ascii_digit() {
                    i += 1;
                }
                if i + 1 < n && chars[i] == '.' && chars[i + 1].is_ascii_digit() {
                    i += 1;
                    while i < n && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                }
                let text: String = chars[start..i].iter().collect();
                let value = text.parse().expect("digits with an optional fraction parse as f64");
                out.push(Token { tok: Tok::Num(value), line });
            }
            'a'..='z' | 'A'..='Z' | '_' => {
                let start = i;
                while i < n && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                let tok = match KEYWORDS.iter().find(|k| **k == word) {
                    Some(kw) => Tok::Sym(kw),
                    None => Tok::Ident(word),
                };
                out.push(Token { tok, line });
            }
            '"' => {
                let start = line;
                let mut text = String::new();
                i += 1;
                loop {
                    if i >= n {
                        return Err(LumenSyntaxError::new(start, "unterminated string"));
                    }
                    let mut c = chars[i];
                    if c == '"' {
                        break;
                    }
                    if c == '\\' {
                        i += 1;
                        if i >= n {
                            return Err(LumenSyntaxError::new(start, "unterminated string"));
                        }
                        c = match chars[i] {
                            'n' => '\n',
                            't' => '\t',
                            '"' => '"',
                            '\\' => '\\',
                            other => {
                                let msg = format!("invalid escape '\\{other}'");
                                return Err(LumenSyntaxError::new(line, msg));
                            }
                        };
                    } else if c == '\n' {
                        line += 1;
                    }
                    text.push(c);
                    i += 1;
                }
                i += 1;
                out.push(Token { tok: Tok::Str(text), line: start });
            }
            _ => {
                let rest: String = chars[i..(i + 2).min(n)].iter().collect();
                match SYMBOLS.iter().find(|s| rest.starts_with(**s)) {
                    Some(sym) => {
                        out.push(Token { tok: Tok::Sym(sym), line });
                        i += sym.len();
                    }
                    None => {
                        let msg = format!("unexpected character '{c}'");
                        return Err(LumenSyntaxError::new(line, msg));
                    }
                }
            }
        }
    }
    out.push(Token { tok: Tok::Eof, line });
    Ok(out)
}

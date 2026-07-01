use std::iter::Peekable;
use std::str::CharIndices;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    Identifier(String),
    Keyword(Keyword),
    At,
    Dot,
    Star,
    Semi,
    Comma,
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Lt,
    Gt,
    Assign,
    Eof,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keyword {
    Package,
    Import,
    Class,
    Interface,
    Enum,
    Record,
    Static,
    Extends,
    Implements,
    Throws,
    Void,
    Return,
    New,
    Public,
    Private,
    Protected,
    Abstract,
    Final,
    Sealed,
    NonSealed,
    Permits,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub line: u32,
    pub column: u32,
}

pub fn lex(content: &str) -> Vec<Token> {
    let mut lexer = Lexer {
        chars: content.char_indices().peekable(),
        line: 1,
        column: 1,
    };
    let mut out = Vec::new();
    loop {
        let tok = lexer.next_token();
        let done = matches!(tok.kind, TokenKind::Eof);
        out.push(tok);
        if done {
            break;
        }
    }
    out
}

struct Lexer<'a> {
    chars: Peekable<CharIndices<'a>>,
    line: u32,
    column: u32,
}

impl<'a> Lexer<'a> {
    fn peek(&mut self) -> Option<char> {
        self.chars.peek().map(|(_, c)| *c)
    }

    fn bump(&mut self) -> Option<char> {
        let (_, c) = self.chars.next()?;
        if c == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        Some(c)
    }

    fn skip_whitespace(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_whitespace() {
                self.bump();
            } else {
                break;
            }
        }
    }

    fn next_token(&mut self) -> Token {
        self.skip_whitespace();
        let line = self.line;
        let column = self.column;
        let Some(c) = self.peek() else {
            return Token {
                kind: TokenKind::Eof,
                line,
                column,
            };
        };

        match c {
            '/' => {
                self.bump();
                match self.peek() {
                    Some('/') => {
                        self.bump();
                        while matches!(self.peek(), Some(x) if x != '\n') {
                            self.bump();
                        }
                        return self.next_token();
                    }
                    Some('*') => {
                        self.bump();
                        while let Some(ch) = self.bump() {
                            if ch == '*' && self.peek() == Some('/') {
                                self.bump();
                                break;
                            }
                        }
                        return self.next_token();
                    }
                    _ => Token {
                        kind: TokenKind::Eof,
                        line,
                        column,
                    },
                }
            }
            '"' => {
                self.bump();
                while let Some(ch) = self.bump() {
                    if ch == '\\' {
                        self.bump();
                        continue;
                    }
                    if ch == '"' {
                        break;
                    }
                }
                self.next_token()
            }
            '\'' => {
                self.bump();
                while let Some(ch) = self.bump() {
                    if ch == '\\' {
                        self.bump();
                        continue;
                    }
                    if ch == '\'' {
                        break;
                    }
                }
                self.next_token()
            }
            '@' => {
                self.bump();
                Token {
                    kind: TokenKind::At,
                    line,
                    column,
                }
            }
            '.' => {
                self.bump();
                Token {
                    kind: TokenKind::Dot,
                    line,
                    column,
                }
            }
            '*' => {
                self.bump();
                Token {
                    kind: TokenKind::Star,
                    line,
                    column,
                }
            }
            ';' => {
                self.bump();
                Token {
                    kind: TokenKind::Semi,
                    line,
                    column,
                }
            }
            ',' => {
                self.bump();
                Token {
                    kind: TokenKind::Comma,
                    line,
                    column,
                }
            }
            '(' => {
                self.bump();
                Token {
                    kind: TokenKind::LParen,
                    line,
                    column,
                }
            }
            ')' => {
                self.bump();
                Token {
                    kind: TokenKind::RParen,
                    line,
                    column,
                }
            }
            '{' => {
                self.bump();
                Token {
                    kind: TokenKind::LBrace,
                    line,
                    column,
                }
            }
            '}' => {
                self.bump();
                Token {
                    kind: TokenKind::RBrace,
                    line,
                    column,
                }
            }
            '[' => {
                self.bump();
                Token {
                    kind: TokenKind::LBracket,
                    line,
                    column,
                }
            }
            ']' => {
                self.bump();
                Token {
                    kind: TokenKind::RBracket,
                    line,
                    column,
                }
            }
            '<' => {
                self.bump();
                Token {
                    kind: TokenKind::Lt,
                    line,
                    column,
                }
            }
            '>' => {
                self.bump();
                Token {
                    kind: TokenKind::Gt,
                    line,
                    column,
                }
            }
            '=' => {
                self.bump();
                Token {
                    kind: TokenKind::Assign,
                    line,
                    column,
                }
            }
            c if c.is_ascii_digit() => {
                self.bump();
                while matches!(self.peek(), Some(x) if x.is_ascii_digit() || x == '.' || x == 'e' || x == 'E' || x == 'f' || x == 'F' || x == 'd' || x == 'D' || x == 'l' || x == 'L') {
                    self.bump();
                }
                self.next_token()
            }
            c if c.is_ascii_alphabetic() || c == '_' || c == '$' => {
                let mut ident = String::new();
                while matches!(self.peek(), Some(x) if x.is_ascii_alphanumeric() || x == '_' || x == '$') {
                    ident.push(self.bump().unwrap());
                }
                let kind = match ident.as_str() {
                    "package" => TokenKind::Keyword(Keyword::Package),
                    "import" => TokenKind::Keyword(Keyword::Import),
                    "class" => TokenKind::Keyword(Keyword::Class),
                    "interface" => TokenKind::Keyword(Keyword::Interface),
                    "enum" => TokenKind::Keyword(Keyword::Enum),
                    "record" => TokenKind::Keyword(Keyword::Record),
                    "static" => TokenKind::Keyword(Keyword::Static),
                    "extends" => TokenKind::Keyword(Keyword::Extends),
                    "implements" => TokenKind::Keyword(Keyword::Implements),
                    "throws" => TokenKind::Keyword(Keyword::Throws),
                    "void" => TokenKind::Keyword(Keyword::Void),
                    "return" => TokenKind::Keyword(Keyword::Return),
                    "new" => TokenKind::Keyword(Keyword::New),
                    "public" => TokenKind::Keyword(Keyword::Public),
                    "private" => TokenKind::Keyword(Keyword::Private),
                    "protected" => TokenKind::Keyword(Keyword::Protected),
                    "abstract" => TokenKind::Keyword(Keyword::Abstract),
                    "final" => TokenKind::Keyword(Keyword::Final),
                    "sealed" => TokenKind::Keyword(Keyword::Sealed),
                    "non-sealed" => TokenKind::Keyword(Keyword::NonSealed),
                    "permits" => TokenKind::Keyword(Keyword::Permits),
                    _ => TokenKind::Identifier(ident),
                };
                Token { kind, line, column }
            }
            _ => {
                self.bump();
                self.next_token()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_class_keyword_inside_string() {
        let tokens = lex(r#"String s = "class Fake {}";"#);
        assert!(!tokens.iter().any(|t| matches!(t.kind, TokenKind::Keyword(Keyword::Class))));
    }

    #[test]
    fn skips_line_comment_before_class() {
        let tokens = lex("// class Ghost\npublic class Real {}");
        assert!(tokens.iter().any(|t| {
            matches!(&t.kind, TokenKind::Identifier(name) if name == "Real")
        }));
    }
}

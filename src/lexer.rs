use std::fmt;

use thiserror::Error;

#[derive(Clone, Debug, PartialEq)]
pub enum TokenKind {
    Ident(String),
    Number(String),
    String(String),
    Pitch(String),
    LBrace,
    RBrace,
    LParen,
    RParen,
    Comma,
    Dot,
    Colon,
    Slash,
    Equal,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub line: usize,
    pub column: usize,
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.column)
    }
}

#[derive(Debug, Error)]
pub enum LexError {
    #[error("{span}: unexpected character `{ch}`")]
    UnexpectedChar { ch: char, span: Span },
    #[error("{span}: unterminated string")]
    UnterminatedString { span: Span },
    #[error("{span}: unterminated block comment")]
    UnterminatedBlockComment { span: Span },
    #[error("{span}: unsupported escape sequence `\\{escape}`")]
    UnsupportedEscape { escape: char, span: Span },
}

pub fn lex(source: &str) -> Result<Vec<Token>, LexError> {
    let mut lexer = Lexer {
        chars: source.chars().collect(),
        pos: 0,
        line: 1,
        column: 1,
        tokens: Vec::new(),
    };
    lexer.lex()?;
    Ok(lexer.tokens)
}

struct Lexer {
    chars: Vec<char>,
    pos: usize,
    line: usize,
    column: usize,
    tokens: Vec<Token>,
}

impl Lexer {
    fn lex(&mut self) -> Result<(), LexError> {
        while let Some(ch) = self.peek() {
            match ch {
                c if c.is_whitespace() => {
                    self.bump();
                }
                '/' if self.peek_next() == Some('/') => self.skip_line_comment(),
                '/' if self.peek_next() == Some('*') => self.skip_block_comment()?,
                '{' => self.push_single(TokenKind::LBrace),
                '}' => self.push_single(TokenKind::RBrace),
                '(' => self.push_single(TokenKind::LParen),
                ')' => self.push_single(TokenKind::RParen),
                ',' => self.push_single(TokenKind::Comma),
                '.' => self.push_single(TokenKind::Dot),
                ':' => self.push_single(TokenKind::Colon),
                '/' => self.push_single(TokenKind::Slash),
                '=' => self.push_single(TokenKind::Equal),
                '"' => self.lex_string()?,
                '-' | '0'..='9' => self.lex_number()?,
                'A'..='G' => self.lex_pitch_or_error()?,
                'a'..='z' | '_' => self.lex_ident(),
                _ => {
                    return Err(LexError::UnexpectedChar {
                        ch,
                        span: self.span(),
                    });
                }
            }
        }
        Ok(())
    }

    fn push_single(&mut self, kind: TokenKind) {
        let span = self.span();
        self.bump();
        self.tokens.push(Token { kind, span });
    }

    fn lex_ident(&mut self) {
        let span = self.span();
        let mut ident = String::new();
        while let Some(ch) = self.peek() {
            if ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' {
                ident.push(ch);
                self.bump();
            } else {
                break;
            }
        }
        self.tokens.push(Token {
            kind: TokenKind::Ident(ident),
            span,
        });
    }

    fn lex_pitch_or_error(&mut self) -> Result<(), LexError> {
        let span = self.span();
        let mut pitch = String::new();
        pitch.push(self.bump().expect("peeked pitch root"));
        if matches!(self.peek(), Some('b' | '#')) {
            pitch.push(self.bump().expect("peeked accidental"));
        }
        let mut has_octave = false;
        while let Some(ch) = self.peek() {
            if ch.is_ascii_digit() {
                has_octave = true;
                pitch.push(ch);
                self.bump();
            } else {
                break;
            }
        }
        if !has_octave {
            return Err(LexError::UnexpectedChar {
                ch: pitch.chars().next().unwrap(),
                span,
            });
        }
        self.tokens.push(Token {
            kind: TokenKind::Pitch(pitch),
            span,
        });
        Ok(())
    }

    fn lex_number(&mut self) -> Result<(), LexError> {
        let span = self.span();
        let mut number = String::new();
        if self.peek() == Some('-') {
            number.push('-');
            self.bump();
        }
        let mut has_digit = false;
        while let Some(ch) = self.peek() {
            if ch.is_ascii_digit() {
                has_digit = true;
                number.push(ch);
                self.bump();
            } else {
                break;
            }
        }
        if self.peek() == Some('.') {
            number.push('.');
            self.bump();
            while let Some(ch) = self.peek() {
                if ch.is_ascii_digit() {
                    has_digit = true;
                    number.push(ch);
                    self.bump();
                } else {
                    break;
                }
            }
        }
        if !has_digit {
            return Err(LexError::UnexpectedChar { ch: '-', span });
        }
        self.tokens.push(Token {
            kind: TokenKind::Number(number),
            span,
        });
        Ok(())
    }

    fn lex_string(&mut self) -> Result<(), LexError> {
        let span = self.span();
        self.bump();
        let mut value = String::new();
        while let Some(ch) = self.peek() {
            match ch {
                '"' => {
                    self.bump();
                    self.tokens.push(Token {
                        kind: TokenKind::String(value),
                        span,
                    });
                    return Ok(());
                }
                '\\' => {
                    self.bump();
                    let escape_span = self.span();
                    let escaped = self.bump().ok_or(LexError::UnterminatedString { span })?;
                    match escaped {
                        '"' => value.push('"'),
                        '\\' => value.push('\\'),
                        'n' => value.push('\n'),
                        't' => value.push('\t'),
                        other => {
                            return Err(LexError::UnsupportedEscape {
                                escape: other,
                                span: escape_span,
                            });
                        }
                    }
                }
                other => {
                    value.push(other);
                    self.bump();
                }
            }
        }
        Err(LexError::UnterminatedString { span })
    }

    fn skip_line_comment(&mut self) {
        while let Some(ch) = self.peek() {
            self.bump();
            if ch == '\n' {
                break;
            }
        }
    }

    fn skip_block_comment(&mut self) -> Result<(), LexError> {
        let span = self.span();
        self.bump();
        self.bump();
        while let Some(ch) = self.peek() {
            if ch == '*' && self.peek_next() == Some('/') {
                self.bump();
                self.bump();
                return Ok(());
            }
            self.bump();
        }
        Err(LexError::UnterminatedBlockComment { span })
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek_next(&self) -> Option<char> {
        self.chars.get(self.pos + 1).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.pos += 1;
        if ch == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        Some(ch)
    }

    fn span(&self) -> Span {
        Span {
            line: self.line,
            column: self.column,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexes_comments_strings_numbers_and_pitches() {
        let tokens = lex(r#"
// line comment
language 0.1
/* block comment */
working "Black \"Circuit\"\n" {
  meter 4/4
  every 1/16
  daemon bass_2 = saw_sub
  spell hats = euclid(3, 8).rotate(-1)
  root F1
  spell bassline = notes "F1 - Gb1"
}
"#)
        .unwrap();

        assert!(matches!(tokens[0].kind, TokenKind::Ident(ref value) if value == "language"));
        assert!(matches!(tokens[1].kind, TokenKind::Number(ref value) if value == "0.1"));
        assert!(
            tokens
                .iter()
                .any(|token| matches!(token.kind, TokenKind::Pitch(ref value) if value == "F1"))
        );
        assert!(tokens.iter().any(
            |token| matches!(token.kind, TokenKind::String(ref value) if value == "F1 - Gb1")
        ));
        assert!(
            tokens.iter().any(
                |token| matches!(token.kind, TokenKind::String(ref value) if value.contains("Black \"Circuit\""))
            )
        );
        assert!(tokens.iter().any(
            |token| matches!(token.kind, TokenKind::Ident(ref value) if value == "bass_2")
        ));
        assert!(tokens.iter().any(|token| matches!(token.kind, TokenKind::LParen)));
        assert!(tokens.iter().any(|token| matches!(token.kind, TokenKind::Comma)));
        assert!(tokens.iter().any(|token| matches!(token.kind, TokenKind::Dot)));
        assert!(tokens.iter().any(
            |token| matches!(token.kind, TokenKind::Number(ref value) if value == "-1")
        ));
    }

    #[test]
    fn rejects_unterminated_block_comment() {
        let error = lex("language 0.1 /* nope").unwrap_err().to_string();
        assert!(error.contains("unterminated block comment"));
    }

    #[test]
    fn rejects_bad_string_escapes() {
        let error = lex(r#"working "bad \r escape""#).unwrap_err().to_string();
        assert!(error.contains("unsupported escape sequence"));
    }
}

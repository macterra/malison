use std::path::Path;

use anyhow::{Result, bail};

use crate::lexer::{Span, Token, TokenKind, lex};

#[derive(Clone, Debug)]
pub struct Working {
    pub name: String,
    pub tempo_bpm: f64,
    pub meter: (u32, u32),
    pub seed: String,
    pub daemons: Vec<Daemon>,
    pub spells: Vec<Spell>,
    pub rites: Vec<Rite>,
    pub evoke_wav: String,
    pub evoke_span: Span,
}

#[derive(Clone, Debug)]
pub struct Daemon {
    pub name: String,
    pub kind: DaemonKind,
    pub sample_path: Option<String>,
    pub params: Vec<Param>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DaemonKind {
    Sample,
    SawSub,
}

#[derive(Clone, Debug)]
pub struct Spell {
    pub name: String,
    pub kind: PatternKind,
    pub body: String,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PatternKind {
    Rhythm,
    Notes,
}

#[derive(Clone, Debug)]
pub struct Rite {
    pub name: String,
    pub bars: u32,
    pub invokes: Vec<Invoke>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Invoke {
    pub daemon: String,
    pub spell: Option<String>,
    pub every: Option<Duration>,
    pub params: Vec<Param>,
    pub span: Span,
    pub source_order: usize,
}

#[derive(Clone, Debug)]
pub struct Param {
    pub name: String,
    pub value: Value,
}

#[derive(Clone, Debug)]
pub enum Value {
    Number(f64),
    String(String),
    Pitch(String),
}

#[derive(Clone, Copy, Debug)]
pub struct Duration {
    pub beats: f64,
}

pub fn parse_source(path: &Path, source: &str) -> Result<Working> {
    let tokens = lex(source)?;
    Parser::new(path, tokens).parse_file()
}

struct Parser<'a> {
    path: &'a Path,
    tokens: Vec<Token>,
    pos: usize,
    invoke_order: usize,
}

impl<'a> Parser<'a> {
    fn new(path: &'a Path, tokens: Vec<Token>) -> Self {
        Self {
            path,
            tokens,
            pos: 0,
            invoke_order: 0,
        }
    }

    fn parse_file(&mut self) -> Result<Working> {
        self.expect_ident("language")?;
        let version = self.expect_number_string()?;
        if version != "0.1" {
            bail!(
                "{}: unsupported language version `{version}`",
                self.path.display()
            );
        }

        self.expect_ident("working")?;
        let name = self.expect_string()?;
        self.expect(TokenKind::LBrace)?;

        let mut tempo_bpm = None;
        let mut meter = None;
        let mut seed = None;
        let mut daemons = Vec::new();
        let mut spells = Vec::new();
        let mut rites = Vec::new();
        let mut evoke_wav = None;
        let mut evoke_span = None;

        while !self.check(TokenKind::RBrace) {
            let keyword = self.expect_ident_any()?;
            match keyword.as_str() {
                "tempo" => {
                    reject_duplicate("tempo", &tempo_bpm)?;
                    let tempo = self.expect_number()?;
                    if self.check_ident("bpm") {
                        self.advance();
                    }
                    tempo_bpm = Some(tempo);
                }
                "meter" => {
                    reject_duplicate("meter", &meter)?;
                    let numerator = self.expect_u32()?;
                    self.expect(TokenKind::Slash)?;
                    let denominator = self.expect_u32()?;
                    meter = Some((numerator, denominator));
                }
                "seed" => {
                    reject_duplicate("seed", &seed)?;
                    seed = Some(self.expect_string()?);
                }
                "daemon" => daemons.push(self.parse_daemon()?),
                "spell" => spells.push(self.parse_spell()?),
                "rite" => rites.push(self.parse_rite()?),
                "evoke" => {
                    reject_duplicate("evoke wav", &evoke_wav)?;
                    evoke_span = Some(self.previous_span());
                    self.expect_ident("wav")?;
                    evoke_wav = Some(self.expect_string()?);
                }
                other => bail!(
                    "{}: unsupported working statement `{other}`",
                    self.location()
                ),
            }
        }
        self.expect(TokenKind::RBrace)?;

        if !self.is_eof() {
            bail!("{}: unexpected tokens after working", self.location());
        }
        if rites.is_empty() {
            bail!("missing `rite`");
        }

        Ok(Working {
            name,
            tempo_bpm: tempo_bpm.ok_or_else(|| anyhow::anyhow!("missing `tempo`"))?,
            meter: meter.ok_or_else(|| anyhow::anyhow!("missing `meter`"))?,
            seed: seed.ok_or_else(|| anyhow::anyhow!("missing `seed`"))?,
            daemons,
            spells,
            rites,
            evoke_wav: evoke_wav.ok_or_else(|| anyhow::anyhow!("missing `evoke wav`"))?,
            evoke_span: evoke_span.ok_or_else(|| anyhow::anyhow!("missing `evoke wav`"))?,
        })
    }

    fn parse_daemon(&mut self) -> Result<Daemon> {
        let span = self.previous_span();
        let name = self.expect_decl_name("daemon")?;
        self.expect(TokenKind::Equal)?;
        let kind_name = self.expect_ident_any()?;
        let kind = match kind_name.as_str() {
            "sample" => DaemonKind::Sample,
            "saw_sub" => DaemonKind::SawSub,
            _ => bail!("{}: unsupported daemon kind `{kind_name}`", self.location()),
        };
        let sample_path = if kind == DaemonKind::Sample {
            Some(self.expect_string()?)
        } else {
            None
        };
        let params = if self.check(TokenKind::LBrace) {
            self.expect(TokenKind::LBrace)?;
            let mut params = Vec::new();
            while !self.check(TokenKind::RBrace) {
                params.push(self.parse_param()?);
            }
            self.expect(TokenKind::RBrace)?;
            params
        } else {
            Vec::new()
        };
        Ok(Daemon {
            name,
            kind,
            sample_path,
            params,
            span,
        })
    }

    fn parse_spell(&mut self) -> Result<Spell> {
        let span = self.previous_span();
        let name = self.expect_decl_name("spell")?;
        self.expect(TokenKind::Equal)?;
        let pattern_kind = self.expect_ident_any()?;
        let kind = match pattern_kind.as_str() {
            "pattern" => PatternKind::Rhythm,
            "notes" => PatternKind::Notes,
            "euclid" => {
                self.expect(TokenKind::LParen)?;
                let pulses = self.expect_u32()?;
                self.expect(TokenKind::Comma)?;
                let steps = self.expect_u32()?;
                self.expect(TokenKind::RParen)?;
                let rotate = if self.check(TokenKind::Dot) {
                    self.advance();
                    self.expect_ident("rotate")?;
                    self.expect(TokenKind::LParen)?;
                    let steps = self.expect_i32()?;
                    self.expect(TokenKind::RParen)?;
                    Some(steps)
                } else {
                    None
                };
                let body = if let Some(rotate) = rotate {
                    format!("euclid({pulses}, {steps}).rotate({rotate})")
                } else {
                    format!("euclid({pulses}, {steps})")
                };
                return Ok(Spell {
                    name,
                    kind: PatternKind::Rhythm,
                    body,
                    span,
                });
            }
            _ => bail!(
                "{}: unsupported spell pattern `{pattern_kind}`",
                self.location()
            ),
        };
        let body = self.expect_string()?;
        Ok(Spell {
            name,
            kind,
            body,
            span,
        })
    }

    fn parse_rite(&mut self) -> Result<Rite> {
        let span = self.previous_span();
        let name = match self.peek_kind() {
            Some(TokenKind::Ident(_)) => self.expect_decl_name("rite")?,
            Some(TokenKind::String(_)) => self.expect_string()?,
            _ => bail!("{}: expected rite name", self.location()),
        };
        self.expect_ident("bars")?;
        let bars = self.expect_u32()?;
        self.expect(TokenKind::LBrace)?;
        let mut invokes = Vec::new();
        while !self.check(TokenKind::RBrace) {
            self.expect_ident("invoke")?;
            invokes.push(self.parse_invoke()?);
        }
        self.expect(TokenKind::RBrace)?;
        Ok(Rite {
            name,
            bars,
            invokes,
            span,
        })
    }

    fn parse_invoke(&mut self) -> Result<Invoke> {
        let span = self.previous_span();
        let daemon = self.expect_ident_any()?;
        let spell = if self.check_ident("with") {
            self.advance();
            Some(self.expect_ident_any()?)
        } else {
            None
        };
        let every = if self.check_ident("every") {
            self.advance();
            Some(self.parse_duration()?)
        } else {
            None
        };
        let mut params = Vec::new();
        while !self.check(TokenKind::RBrace) && !self.check_ident("invoke") {
            params.push(self.parse_param()?);
        }
        let source_order = self.invoke_order;
        self.invoke_order += 1;
        Ok(Invoke {
            daemon,
            spell,
            every,
            params,
            span,
            source_order,
        })
    }

    fn parse_param(&mut self) -> Result<Param> {
        let name = self.expect_ident_any()?;
        let value = match self.peek_kind() {
            Some(TokenKind::Number(_)) => Value::Number(self.expect_number()?),
            Some(TokenKind::String(_)) => Value::String(self.expect_string()?),
            Some(TokenKind::Pitch(_)) => Value::Pitch(self.expect_pitch()?),
            Some(TokenKind::Ident(_)) => Value::String(self.expect_ident_any()?),
            _ => bail!("{}: expected value for parameter `{name}`", self.location()),
        };
        Ok(Param { name, value })
    }

    fn parse_duration(&mut self) -> Result<Duration> {
        let numerator = self.expect_number()?;
        if self.check(TokenKind::Slash) {
            self.advance();
            let denominator = self.expect_number()?;
            return Ok(Duration {
                beats: 4.0 * numerator / denominator,
            });
        }

        let unit = match self.peek_kind() {
            Some(TokenKind::Ident(unit))
                if matches!(unit.as_str(), "beat" | "beats" | "bars" | "sec") =>
            {
                let unit = unit.clone();
                self.advance();
                unit
            }
            _ => return Ok(Duration { beats: numerator }),
        };

        let beats = match unit.as_str() {
            "beat" | "beats" => numerator,
            "bars" => bail!(
                "{}: bar durations are not valid for `every`",
                self.location()
            ),
            "sec" => bail!(
                "{}: second durations need tempo lowering, not yet supported",
                self.location()
            ),
            _ => bail!("{}: unsupported duration unit `{unit}`", self.location()),
        };
        Ok(Duration { beats })
    }

    fn expect(&mut self, expected: TokenKind) -> Result<()> {
        let eof = self.eof_location();
        let token = self
            .advance()
            .ok_or_else(|| anyhow::anyhow!("{}: expected {:?}", eof, expected))?;
        if same_variant(&token.kind, &expected) {
            Ok(())
        } else {
            bail!(
                "{}: expected {:?}, found {:?}",
                token.span,
                expected,
                token.kind
            )
        }
    }

    fn expect_ident(&mut self, expected: &str) -> Result<()> {
        let span = self.location();
        let ident = self.expect_ident_any()?;
        if ident == expected {
            Ok(())
        } else {
            bail!("{span}: expected `{expected}`, found `{ident}`")
        }
    }

    fn expect_ident_any(&mut self) -> Result<String> {
        match self.advance() {
            Some(Token {
                kind: TokenKind::Ident(ident),
                ..
            }) => Ok(ident.clone()),
            Some(token) => bail!(
                "{}: expected identifier, found {:?}",
                token.span,
                token.kind
            ),
            None => bail!("{}: expected identifier", self.eof_location()),
        }
    }

    fn expect_decl_name(&mut self, kind: &str) -> Result<String> {
        let span = self.location();
        let name = self.expect_ident_any()?;
        if is_reserved_word(&name) {
            bail!("{span}: reserved word `{name}` cannot be used as a {kind} name");
        }
        Ok(name)
    }

    fn expect_string(&mut self) -> Result<String> {
        match self.advance() {
            Some(Token {
                kind: TokenKind::String(value),
                ..
            }) => Ok(value.clone()),
            Some(token) => bail!("{}: expected string, found {:?}", token.span, token.kind),
            None => bail!("{}: expected string", self.eof_location()),
        }
    }

    fn expect_pitch(&mut self) -> Result<String> {
        match self.advance() {
            Some(Token {
                kind: TokenKind::Pitch(value),
                ..
            }) => Ok(value.clone()),
            Some(token) => bail!("{}: expected pitch, found {:?}", token.span, token.kind),
            None => bail!("{}: expected pitch", self.eof_location()),
        }
    }

    fn expect_number(&mut self) -> Result<f64> {
        let eof = self.eof_location();
        let token = self
            .advance()
            .ok_or_else(|| anyhow::anyhow!("{}: expected number", eof))?;
        if let TokenKind::Number(value) = &token.kind {
            Ok(value.parse()?)
        } else {
            bail!("{}: expected number, found {:?}", token.span, token.kind)
        }
    }

    fn expect_number_string(&mut self) -> Result<String> {
        match self.advance() {
            Some(Token {
                kind: TokenKind::Number(value),
                ..
            }) => Ok(value.clone()),
            Some(token) => bail!("{}: expected number, found {:?}", token.span, token.kind),
            None => bail!("{}: expected number", self.eof_location()),
        }
    }

    fn expect_u32(&mut self) -> Result<u32> {
        let number = self.expect_number()?;
        if number.fract() != 0.0 || number < 0.0 {
            bail!("{}: expected unsigned integer", self.previous_span());
        }
        Ok(number as u32)
    }

    fn expect_i32(&mut self) -> Result<i32> {
        let number = self.expect_number()?;
        if number.fract() != 0.0 {
            bail!("{}: expected integer", self.previous_span());
        }
        Ok(number as i32)
    }

    fn check(&self, expected: TokenKind) -> bool {
        self.peek_kind()
            .is_some_and(|kind| same_variant(kind, &expected))
    }

    fn check_ident(&self, expected: &str) -> bool {
        matches!(self.peek_kind(), Some(TokenKind::Ident(ident)) if ident == expected)
    }

    fn peek_kind(&self) -> Option<&TokenKind> {
        self.tokens.get(self.pos).map(|token| &token.kind)
    }

    fn advance(&mut self) -> Option<&Token> {
        let token = self.tokens.get(self.pos)?;
        self.pos += 1;
        Some(token)
    }

    fn is_eof(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    fn location(&self) -> Span {
        self.tokens
            .get(self.pos)
            .map(|token| token.span)
            .unwrap_or_else(|| self.eof_location())
    }

    fn previous_span(&self) -> Span {
        self.tokens
            .get(self.pos.saturating_sub(1))
            .map(|token| token.span)
            .unwrap_or_else(|| self.eof_location())
    }

    fn eof_location(&self) -> Span {
        self.tokens
            .last()
            .map(|token| token.span)
            .unwrap_or(Span { line: 1, column: 1 })
    }
}

fn same_variant(a: &TokenKind, b: &TokenKind) -> bool {
    std::mem::discriminant(a) == std::mem::discriminant(b)
}

fn reject_duplicate<T>(name: &str, slot: &Option<T>) -> Result<()> {
    if slot.is_some() {
        bail!("duplicate `{name}` declaration");
    }
    Ok(())
}

fn is_reserved_word(value: &str) -> bool {
    matches!(
        value,
        "language"
            | "working"
            | "tempo"
            | "meter"
            | "seed"
            | "daemon"
            | "sample"
            | "saw_sub"
            | "spell"
            | "pattern"
            | "notes"
            | "rite"
            | "bars"
            | "invoke"
            | "with"
            | "every"
            | "evoke"
            | "wav"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_v01_working() {
        let source = r#"
language 0.1

working "First Working" {
  tempo 128
  meter 4/4
  seed "first"

  daemon kick = sample "samples/kick.wav" {
    gain -3
  }

  daemon bass = saw_sub {
    cutoff 300
    drive 0.3
  }

  spell kicks = pattern "x---"
  spell bassline = notes "F1 - Gb1 -"

  rite main bars 1 {
    invoke kick with kicks every 1/16
    invoke bass with bassline every 1/8
  }

  evoke wav "renders/first-working.wav"
}
"#;

        let working = parse_source(Path::new("fixture.rite"), source).unwrap();
        assert_eq!(working.name, "First Working");
        assert_eq!(working.daemons.len(), 2);
        assert_eq!(working.spells.len(), 2);
        assert_eq!(working.rites.len(), 1);
    }

    #[test]
    fn parses_quoted_rite_name() {
        let source = r#"
language 0.1

working "Quoted Rite" {
  tempo 120
  meter 4/4
  seed "seed"

  daemon kick = sample "samples/kick.wav"
  spell kicks = pattern "x---"

  rite "machine prayer" bars 1 {
    invoke kick with kicks every 1/16
  }

  evoke wav "renders/quoted.wav"
}
"#;

        let working = parse_source(Path::new("fixture.rite"), source).unwrap();
        assert_eq!(working.rites[0].name, "machine prayer");
    }

    #[test]
    fn rejects_reserved_future_syntax_in_rite_body() {
        let source = r#"
language 0.1

working "Future Syntax" {
  tempo 120
  meter 4/4
  seed "seed"

  daemon kick = sample "samples/kick.wav"

  rite main bars 1 {
    raise tension 0.1
  }

  evoke wav "renders/future.wav"
}
"#;

        let error = parse_source(Path::new("fixture.rite"), source)
            .unwrap_err()
            .to_string();
        assert!(error.contains("expected `invoke`, found `raise`"));
    }

    #[test]
    fn rejects_reserved_words_as_declaration_names() {
        let source = r#"
language 0.1

working "Reserved Names" {
  tempo 120
  meter 4/4
  seed "seed"

  daemon invoke = sample "samples/kick.wav"
  spell kicks = pattern "x---"

  rite main bars 1 {
    invoke invoke with kicks every 1/16
  }

  evoke wav "renders/reserved.wav"
}
"#;

        let error = parse_source(Path::new("fixture.rite"), source)
            .unwrap_err()
            .to_string();
        assert!(error.contains("reserved word `invoke` cannot be used as a daemon name"));
    }
}

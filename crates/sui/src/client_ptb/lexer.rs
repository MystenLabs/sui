// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::sp;

use super::{
    error::{Span, Spanned},
    token::{Lexeme, Token as T},
};

pub struct Lexer<'l, I: Iterator<Item = &'l str>> {
    pub buf: &'l str,
    pub tokens: I,
    pub offset: usize,
    pub done: Option<Spanned<Lexeme<'l>>>,
}

impl<'l, I: Iterator<Item = &'l str>> Lexer<'l, I> {
    pub fn new(mut tokens: I) -> Option<Self> {
        let buf = tokens.next()?;

        Some(Self {
            buf,
            tokens,
            offset: 0,
            done: None,
        })
    }

    /// Returns the next character in the current shell token, along with the byte offset it ends
    /// at, or None if the current shell token is empty.
    fn next_char_boundary(&self) -> Option<(usize, char)> {
        let mut chars = self.buf.char_indices();
        let (_, c) = chars.next()?;
        let ix = chars.next().map_or(self.buf.len(), |(ix, _)| ix);
        Some((ix, c))
    }

    /// Repeatedly consume whitespace, stopping only if you hit a non-whitespace character, or the
    /// end of the shell token stream.
    fn eat_whitespace(&mut self) {
        loop {
            if let Some((ix, c)) = self.next_char_boundary() {
                if c.is_whitespace() {
                    self.buf = &self.buf[ix..];
                    self.offset += ix;
                } else {
                    break;
                }
            } else if let Some(next) = self.tokens.next() {
                self.offset += 1; // +1 for the space between tokens
                self.buf = next;
            } else {
                break;
            };
        }
    }

    /// Checks whether the current shell token starts with the prefix `patt`, and consumes it if so,
    /// returning a spanned slice of the consumed prefix.
    fn eat_prefix(&mut self, patt: &str) -> Option<Spanned<&'l str>> {
        let start = self.offset;

        let rest = self.buf.strip_prefix(patt)?;

        let len = self.buf.len() - rest.len();
        let value = &self.buf[..len];
        self.offset += len;
        self.buf = rest;

        let span = Span {
            start,
            end: self.offset,
        };
        Some(Spanned { span, value })
    }

    /// Checks whether the current shell token starts with at least one character that satisfies
    /// `pred`. Consumes all such characters from the front of the shell token, returning a spanned
    /// slice of the consumed prefix.
    fn eat_while(&mut self, pred: impl FnMut(char) -> bool) -> Option<Spanned<&'l str>> {
        let start = self.offset;

        let rest = self.buf.trim_start_matches(pred);
        if self.buf == rest {
            return None;
        };

        let len = self.buf.len() - rest.len();
        let value = &self.buf[..len];
        self.offset += len;
        self.buf = rest;

        let span = Span {
            start,
            end: self.offset,
        };
        Some(Spanned { span, value })
    }

    /// Consume the whole next shell token (assumes the current shell token has already been
    /// consumed).
    fn eat_token(&mut self) -> Option<Spanned<&'l str>> {
        debug_assert!(self.buf.is_empty());
        let start = self.offset + 1;
        let value = self.tokens.next()?;
        self.offset += value.len() + 1;

        let span = Span {
            start,
            end: self.offset,
        };
        Some(Spanned { span, value })
    }

    /// Look at the next character in the current shell token without consuming it, if it exists.
    fn peek(&self) -> Option<Spanned<&'l str>> {
        let start = self.offset;
        let (ix, _) = self.next_char_boundary()?;

        let value = &self.buf[..ix];
        let span = Span {
            start,
            end: start + ix,
        };
        Some(Spanned { span, value })
    }

    /// Consume the next character in the current shell token, assuming there is one.
    fn bump(&mut self) {
        if let Some((ix, _)) = self.next_char_boundary() {
            self.buf = &self.buf[ix..];
            self.offset += ix;
        }
    }

    /// Tokenize a string at the prefix of the current shell token. `start` is the spanned slice
    /// containing the initial quote character, which also specifies the terminating quote
    /// character.
    ///
    /// A string that is not terminated in the same shell token it was started in is tokenized as an
    /// `UnfinishedString`, even if it would have been terminated in a following shell token.
    fn string(&mut self, start: Spanned<&'l str>) -> Spanned<Lexeme<'l>> {
        self.bump();
        let sp!(sp, quote) = start;

        let mut escaped = false;
        let content = self
            .eat_while(|c| {
                if escaped {
                    escaped = false;
                    true
                } else if c == '\\' {
                    escaped = true;
                    true
                } else {
                    !quote.starts_with(c)
                }
            })
            .unwrap_or(Spanned {
                span: Span {
                    start: sp.end,
                    end: sp.end,
                },
                value: "",
            })
            .widen(start);

        let Some(end) = self.eat_prefix(quote) else {
            let error = content.map(|src| Lexeme(T::UnfinishedString, src));
            self.done = Some(error);
            return error;
        };

        content.widen(end).map(|src| Lexeme(T::String, src))
    }

    /// Signal that `c` is an unexpected token, and trigger the lexer's error flag, to prevent
    /// further iteration.
    fn unexpected(&mut self, c: Spanned<&'l str>) -> Spanned<Lexeme<'l>> {
        let error = c.map(|src| Lexeme(T::Unexpected, src));
        self.done = Some(error);
        error
    }

    /// Signal that the lexer has experienced an unexpected, early end-of-file, and trigger the
    /// lexer's error flag, to prevent further iteration.
    fn done(&mut self, token: T) -> Spanned<Lexeme<'l>> {
        let error = self.offset().wrap(Lexeme(token, ""));
        self.done = Some(error);
        error
    }

    /// Span pointing to the current offset in the input.
    fn offset(&self) -> Span {
        Span {
            start: self.offset,
            end: self.offset,
        }
    }
}

impl<'l, I: Iterator<Item = &'l str>> Iterator for Lexer<'l, I> {
    type Item = Spanned<Lexeme<'l>>;

    fn next(&mut self) -> Option<Self::Item> {
        // Lexer has been expended, repeatedly return the terminal token.
        if let Some(done) = self.done {
            return Some(done);
        }

        self.eat_whitespace();

        let Some(c) = self.peek() else {
            return Some(self.done(T::Eof));
        };

        macro_rules! token {
            ($t:expr) => {{
                self.bump();
                c.map(|src| Lexeme($t, src))
            }};
        }

        Some(match c {
            // Single character tokens
            sp!(_, ",") => token!(T::Comma),
            sp!(_, "[") => token!(T::LBracket),
            sp!(_, "]") => token!(T::RBracket),
            sp!(_, "(") => token!(T::LParen),
            sp!(_, ")") => token!(T::RParen),
            sp!(_, "<") => token!(T::LAngle),
            sp!(_, ">") => token!(T::RAngle),
            sp!(_, "@") => token!(T::At),
            sp!(_, ".") => token!(T::Dot),

            sp!(_, "'" | "\"") => self.string(c),

            sp!(_, ":") => 'colon: {
                let Some(sp) = self.eat_prefix("::") else {
                    break 'colon self.unexpected(c);
                };

                sp.map(|src| Lexeme(T::ColonColon, src))
            }

            sp!(_, c) if c.chars().next().is_some_and(is_ident_start) => {
                let Some(ident) = self.eat_while(is_ident_continue) else {
                    unreachable!("is_ident_start implies is_ident_continue");
                };

                ident.map(|src| Lexeme(T::Ident, src))
            }

            sp!(_, "0") => 'zero: {
                let Some(prefix) = self.eat_prefix("0x") else {
                    break 'zero token!(T::Number);
                };

                let Some(digits) = self.eat_while(is_hex_continue) else {
                    break 'zero self.unexpected(prefix);
                };

                digits.widen(prefix).map(|src| Lexeme(T::HexNumber, src))
            }

            sp!(_, n) if n.chars().next().is_some_and(is_number_start) => {
                let Some(num) = self.eat_while(is_number_continue) else {
                    unreachable!("is_number_start implies is_number_continue");
                };

                num.map(|src| Lexeme(T::Number, src))
            }

            sp!(_, "-") => 'command: {
                self.bump();
                let Some(next) = self.peek() else {
                    break 'command self.unexpected(c);
                };

                match next {
                    sp!(_, "-") => {
                        self.bump();
                    }
                    sp!(_, flag) if is_flag(flag.chars().next().unwrap()) => {
                        self.bump();
                        break 'command next.widen(c).map(|src| Lexeme(T::Flag, src));
                    }
                    sp!(_, _) => break 'command self.unexpected(next),
                }

                let Some(ident) = self.eat_while(is_ident_continue) else {
                    break 'command self.unexpected(c);
                };

                match ident {
                    sp!(_, "publish") => {
                        if let Some(next) = self.peek() {
                            break 'command self.unexpected(next);
                        }

                        let Some(file) = self.eat_token() else {
                            break 'command self.done(T::EarlyEof);
                        };

                        file.widen(c).map(|src| Lexeme(T::Publish, src))
                    }

                    sp!(_, "upgrade") => {
                        if let Some(next) = self.peek() {
                            break 'command self.unexpected(next);
                        }

                        let Some(file) = self.eat_token() else {
                            break 'command self.done(T::EarlyEof);
                        };

                        file.widen(c).map(|src| Lexeme(T::Upgrade, src))
                    }

                    sp!(_, _) => ident.widen(c).map(|src| Lexeme(T::Command, src)),
                }
            }

            sp!(_, _) => self.unexpected(c),
        })
    }
}

fn is_flag(c: char) -> bool {
    c.is_ascii_alphanumeric()
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_ident_continue(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

fn is_number_start(c: char) -> bool {
    c.is_ascii_digit()
}

fn is_number_continue(c: char) -> bool {
    c.is_ascii_digit() || c == '_'
}

fn is_hex_continue(c: char) -> bool {
    c.is_ascii_hexdigit() || c == '_'
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tokenize the input up to and including the first terminal token.
    fn lex(input: Vec<&str>) -> Vec<Spanned<Lexeme>> {
        let mut lexer = Lexer::new(input.into_iter()).unwrap();
        let mut lexemes: Vec<_> = (&mut lexer)
            .take_while(|sp!(_, lex)| !lex.is_terminal())
            .collect();
        lexemes.push(lexer.next().unwrap());
        lexemes
    }

    #[test]
    fn tokenize_vector() {
        let vecs = vec![
            "vector[1,2,3]",
            "vector[1, 2, 3]",
            "vector[]",
            "vector[1]",
            "vector[1,]",
        ];

        insta::assert_debug_snapshot!(lex(vecs));
    }

    #[test]
    fn tokenize_array() {
        let arrays = vec!["[1,2,3]", "[1, 2, 3]", "[]", "[1]", "[1,]"];
        insta::assert_debug_snapshot!(lex(arrays));
    }

    #[test]
    fn tokenize_num() {
        let nums = vec![
            "1",
            "1_000",
            "100_000_000",
            "100_000u64",
            "1u8",
            "1_u128",
            "0x1",
            "0x1_000",
            "0x100_000_000",
            "0x100_000u64",
            "0x1u8",
            "0x1_u128",
        ];

        insta::assert_debug_snapshot!(lex(nums));
    }

    #[test]
    fn tokenize_address() {
        let addrs = vec![
            "@0x1",
            "@0x1_000",
            "@0x100_000_000",
            "@0x100_000u64",
            "@0x1u8",
            "@0x1_u128",
        ];

        insta::assert_debug_snapshot!(lex(addrs));
    }

    #[test]
    fn tokenize_commands() {
        let cmds = vec!["--f00", "--Bar_baz", "--qux-quy"];

        insta::assert_debug_snapshot!(lex(cmds));
    }

    #[test]
    fn tokenize_flags() {
        let flags = vec!["-h", "-a", "-Z", "-1"];

        insta::assert_debug_snapshot!(lex(flags));
    }

    #[test]
    fn tokenize_args() {
        let args = vec![
            "@0x1 1 1u8 1_u128 1_000 100_000_000 100_000u64 1 [1, 2, 3, 4, 5, 6, 7, 8, 9, 10] \
             vector[] vector[1,2,3] vector[1]",
            "some(@0x1) none some(vector[1,2,3]) --assign --transfer-objects --split-coins \
             --merge-coins --make-move-vec --move-call --preview --warn-shadows --pick-gas-budget \
             --gas-budget --summary",
            "--publish",
            "package-a",
            "--upgrade",
            "package-b",
        ];

        insta::assert_debug_snapshot!(lex(args));
    }

    #[test]
    fn dotted_idents() {
        let idents = vec!["a", "a.b", "a.b.c", "a.b.c.d", "a.b.c.d.e"];
        insta::assert_debug_snapshot!(lex(idents));
    }

    #[test]
    fn gas() {
        let gas = vec!["gas"];
        insta::assert_debug_snapshot!(lex(gas));
    }

    #[test]
    fn functions() {
        let funs = vec![
            "0x2::transfer::public_transfer<0x42::foo::Bar>",
            "std::option::is_none<u64>",
            "0x1::option::is_some <u64>",
            "0x1::option::is_none",
            "<u64>",
        ];

        insta::assert_debug_snapshot!(lex(funs));
    }

    #[test]
    fn unexpected_colon() {
        let unexpected = vec!["hello: world"];
        insta::assert_debug_snapshot!(lex(unexpected));
    }

    #[test]
    fn unexpected_0x() {
        let unexpected = vec!["0x forgot my train of thought"];
        insta::assert_debug_snapshot!(lex(unexpected));
    }

    #[test]
    fn unexpected_dash() {
        let unexpected = vec!["-"];
        insta::assert_debug_snapshot!(lex(unexpected));
    }

    #[test]
    fn unexpected_dash_dash() {
        let unexpected = vec!["--"];
        insta::assert_debug_snapshot!(lex(unexpected));
    }

    #[test]
    fn unexpected_publish_trailing() {
        let unexpected = vec!["--publish needs a token break"];
        insta::assert_debug_snapshot!(lex(unexpected));
    }

    #[test]
    fn unexpected_upgrade_eof() {
        let unexpected = vec!["--upgrade"]; // needs a next token
        insta::assert_debug_snapshot!(lex(unexpected));
    }

    #[test]
    fn unexpected_random_chars() {
        let unexpected = vec!["4 * 5"];
        insta::assert_debug_snapshot!(lex(unexpected));
    }
}

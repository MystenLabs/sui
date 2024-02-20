// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::sp_;

use super::{
    error::{Span, Spanned},
    token::{Lexeme, Token},
};

pub struct Lexer<'l, I: Iterator<Item = &'l str>> {
    pub buf: &'l str,
    pub tokens: I,
    pub offset: usize,
    pub errored: bool,
}

impl<'l, I: Iterator<Item = &'l str>> Lexer<'l, I> {
    pub fn new(mut tokens: I) -> Option<Self> {
        let Some(buf) = tokens.next() else {
            return None;
        };

        Some(Self {
            buf,
            tokens,
            offset: 0,
            errored: false,
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

        let Some(rest) = self.buf.strip_prefix(patt) else {
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
        let Some((ix, _)) = self.next_char_boundary() else {
            return None;
        };

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
        let sp_!(sp, quote) = &start;

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
            });

        let Some(end) = self.eat_prefix(quote) else {
            return content.widen(start).map(|src| Lexeme {
                token: Token::UnfinishedString,
                src,
            });
        };

        content.widen(start).widen(end).map(|src| Lexeme {
            token: Token::String,
            src,
        })
    }

    /// Signal that `c` is an unexpected token, and trigger the lexer's error flag, to prevent
    /// further iteration.
    fn unexpected(&mut self, c: Spanned<&'l str>) -> Spanned<Lexeme<'l>> {
        self.errored = true;
        c.map(|src| Lexeme {
            token: Token::Unexpected,
            src,
        })
    }

    /// Signal that the lexer has experienced an unexpected, early end-of-file, and trigger the
    /// lexer's error flag, to prevent further iteration.
    fn early_eof(&mut self) -> Spanned<Lexeme<'l>> {
        self.errored = true;
        Spanned {
            span: Span {
                start: self.offset,
                end: self.offset,
            },
            value: Lexeme {
                token: Token::EarlyEof,
                src: "",
            },
        }
    }
}

impl<'l, I: Iterator<Item = &'l str>> Iterator for Lexer<'l, I> {
    type Item = Spanned<Lexeme<'l>>;

    fn next(&mut self) -> Option<Self::Item> {
        use Token as T;

        // Lexer cannot be restarted after hitting an error.
        if self.errored {
            return None;
        }

        self.eat_whitespace();

        let Some(c) = self.peek() else {
            return None;
        };

        macro_rules! token {
            ($t:expr) => {{
                self.bump();
                c.map(|src| Lexeme { token: $t, src })
            }};
        }

        Some(match c {
            // Single character tokens
            sp_!(_, ",") => token!(T::Comma),
            sp_!(_, "[") => token!(T::LBracket),
            sp_!(_, "]") => token!(T::RBracket),
            sp_!(_, "(") => token!(T::LParen),
            sp_!(_, ")") => token!(T::RParen),
            sp_!(_, "<") => token!(T::LAngle),
            sp_!(_, ">") => token!(T::RAngle),
            sp_!(_, "@") => token!(T::At),
            sp_!(_, ".") => token!(T::Dot),

            sp_!(_, "'" | "\"") => self.string(c),

            sp_!(_, ":") => 'colon: {
                let Some(sp) = self.eat_prefix("::") else {
                    break 'colon self.unexpected(c);
                };

                sp.map(|src| Lexeme {
                    token: T::ColonColon,
                    src,
                })
            }

            sp_!(_, c) if c.chars().next().is_some_and(is_ident_start) => {
                let Some(ident) = self.eat_while(is_ident_continue) else {
                    unreachable!("is_ident_start implies is_ident_continue");
                };

                ident.map(|src| Lexeme {
                    token: T::Ident,
                    src,
                })
            }

            sp_!(_, "0") => 'zero: {
                let Some(prefix) = self.eat_prefix("0x") else {
                    break 'zero token!(T::Number);
                };

                let Some(digits) = self.eat_while(is_hex_continue) else {
                    break 'zero self.unexpected(prefix);
                };

                digits.widen(prefix).map(|src| Lexeme {
                    token: T::HexNumber,
                    src,
                })
            }

            sp_!(_, n) if n.chars().next().is_some_and(is_number_start) => {
                let Some(num) = self.eat_while(is_number_continue) else {
                    unreachable!("is_number_start implies is_number_continue");
                };

                num.map(|src| Lexeme {
                    token: T::Number,
                    src,
                })
            }

            sp_!(_, "-") => 'command: {
                let Some(prefix) = self.eat_prefix("--") else {
                    break 'command self.unexpected(c);
                };

                let Some(ident) = self.eat_while(is_ident_continue) else {
                    break 'command self.unexpected(prefix);
                };

                match ident {
                    sp_!(_, "publish") => {
                        if let Some(next) = self.peek() {
                            break 'command self.unexpected(next);
                        }

                        let Some(file) = self.eat_token() else {
                            break 'command self.early_eof();
                        };

                        file.widen(prefix).map(|src| Lexeme {
                            token: T::Publish,
                            src,
                        })
                    }

                    sp_!(_, "upgrade") => {
                        if let Some(next) = self.peek() {
                            break 'command self.unexpected(next);
                        }

                        let Some(file) = self.eat_token() else {
                            break 'command self.early_eof();
                        };

                        file.widen(prefix).map(|src| Lexeme {
                            token: T::Upgrade,
                            src,
                        })
                    }

                    sp_!(_, _) => ident.widen(prefix).map(|src| Lexeme {
                        token: T::Command,
                        src,
                    }),
                }
            }

            sp_!(_, _) => self.unexpected(c),
        })
    }
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

    #[test]
    fn tokenize_vector() {
        let vecs = vec![
            "vector[1,2,3]",
            "vector[1, 2, 3]",
            "vector[]",
            "vector[1]",
            "vector[1,]",
        ];

        let tokens: Vec<_> = Lexer::new(vecs.into_iter()).unwrap().collect();
        insta::assert_debug_snapshot!(tokens);
    }

    #[test]
    fn tokenize_array() {
        let arrays = vec!["[1,2,3]", "[1, 2, 3]", "[]", "[1]", "[1,]"];

        let tokens: Vec<_> = Lexer::new(arrays.into_iter()).unwrap().collect();
        insta::assert_debug_snapshot!(tokens);
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

        let tokens: Vec<_> = Lexer::new(nums.into_iter()).unwrap().collect();
        insta::assert_debug_snapshot!(tokens);
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

        let tokens: Vec<_> = Lexer::new(addrs.into_iter()).unwrap().collect();
        insta::assert_debug_snapshot!(tokens);
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

        let tokens: Vec<_> = Lexer::new(args.into_iter()).unwrap().collect();
        insta::assert_debug_snapshot!(tokens);
    }

    #[test]
    fn dotted_idents() {
        let idents = vec!["a", "a.b", "a.b.c", "a.b.c.d", "a.b.c.d.e"];

        let tokens: Vec<_> = Lexer::new(idents.into_iter()).unwrap().collect();
        insta::assert_debug_snapshot!(tokens);
    }

    #[test]
    fn gas() {
        let gas = vec!["gas"];

        let tokens: Vec<_> = Lexer::new(gas.into_iter()).unwrap().collect();
        insta::assert_debug_snapshot!(tokens);
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

        let tokens: Vec<_> = Lexer::new(funs.into_iter()).unwrap().collect();
        insta::assert_debug_snapshot!(tokens);
    }

    #[test]
    fn unexpected_colon() {
        let unexpected = vec!["hello: world"];

        let tokens: Vec<_> = Lexer::new(unexpected.into_iter()).unwrap().collect();
        insta::assert_debug_snapshot!(tokens);
    }

    #[test]
    fn unexpected_0x() {
        let unexpected = vec!["0x forgot my train of thought"];

        let tokens: Vec<_> = Lexer::new(unexpected.into_iter()).unwrap().collect();
        insta::assert_debug_snapshot!(tokens);
    }

    #[test]
    fn unexpected_dash() {
        let unexpected = vec!["-"];

        let tokens: Vec<_> = Lexer::new(unexpected.into_iter()).unwrap().collect();
        insta::assert_debug_snapshot!(tokens);
    }

    #[test]
    fn unexpected_dash_dash() {
        let unexpected = vec!["--"];

        let tokens: Vec<_> = Lexer::new(unexpected.into_iter()).unwrap().collect();
        insta::assert_debug_snapshot!(tokens);
    }

    #[test]
    fn unexpected_publish_trailing() {
        let unexpected = vec!["--publish needs a token break"];

        let tokens: Vec<_> = Lexer::new(unexpected.into_iter()).unwrap().collect();
        insta::assert_debug_snapshot!(tokens);
    }

    #[test]
    fn unexpected_upgrade_eof() {
        let unexpected = vec!["--upgrade"]; // needs a next token

        let tokens: Vec<_> = Lexer::new(unexpected.into_iter()).unwrap().collect();
        insta::assert_debug_snapshot!(tokens);
    }

    #[test]
    fn unexpected_random_chars() {
        let unexpected = vec!["4 * 5"];

        let tokens: Vec<_> = Lexer::new(unexpected.into_iter()).unwrap().collect();
        insta::assert_debug_snapshot!(tokens);
    }
}

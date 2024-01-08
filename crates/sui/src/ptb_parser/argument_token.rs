// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt::{self, Display};

use anyhow::bail;
use move_command_line_common::parser::Token;
use move_core_types::identifier;

// number :=
//     <digit> (<digit>|"_")*       // Allow _ separators in numbers
//     "0x" <digit> (<digit>|"_")*  // Allow hex-encoded numbers
//
// value :=
//     "true"
//     "false"                                        // bool(s)
//     <number><type_suffix>                          // u8, u16, u32, ...
//     "@" <number>                                   // address or sui::object::ID
//     "\"" <text> "\""                               // string
//     "'" <text> "'"                                 // also string
//     "vector" "[" (<value> ",")* <value> (","?) "]" // Move vector -- will be a pure argument to the PTB
//     "[" (<value> ",")* <value> (","?) "]"          // array for a PTB command
//     "some" "(" <value> ")"                         // option::some
//     "none"                                         // option::none
//
// arg :=
//     <value>
//     <ident> // input
//     <ident> ("." <digit>)+  // result access
//     "<arg> <arg>*"          // space separated args
//     "tx" "." "sender"       // sender
//     "tx" "." "gas"          // gas coin
//
// ty :=
//     <prim_type> // u64, bool, u32, address (etc)
//     <addr>::<ident>::<ident> ("<" (<ty>,)+ ">")?
//
// ty_arg :=
//     "<" (<ty>,)+ ">"

#[derive(Eq, PartialEq, Debug, Clone, Copy)]
pub enum ArgumentToken {
    // any whitespace
    Whitespace,
    // alpha numeric
    Ident,
    // digits
    Number,
    // Digits with a type suffix
    NumberTyped,
    // ::
    ColonColon,
    // :
    Colon,
    // ,
    Comma,
    // [
    LBracket,
    // ]
    RBracket,
    // (
    LParen,
    // )
    RParen,
    // vector
    Vector,
    // some
    Some_,
    // none
    None_,
    // "
    DoubleQuote,
    // '
    SingleQuote,
    // @
    At,
    // .
    Dot,
    // <...>
    // eats the whole string, including the < and >, to pass to a different parser
    TypeArgString,
    // input
    Input,
    // result
    Result,
    // gas
    Gas,
    Void,
}

impl Display for ArgumentToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        let s = match *self {
            ArgumentToken::Whitespace => "[whitespace]",
            ArgumentToken::Void => "[void]",
            ArgumentToken::Ident => "[ident]",
            ArgumentToken::Number => "[number]",
            ArgumentToken::NumberTyped => "[number_typed]",
            ArgumentToken::ColonColon => "::",
            ArgumentToken::Colon => ":",
            ArgumentToken::Comma => ",",
            ArgumentToken::LBracket => "[",
            ArgumentToken::RBracket => "]",
            ArgumentToken::LParen => "(",
            ArgumentToken::RParen => ")",
            ArgumentToken::Vector => "vector",
            ArgumentToken::Some_ => "some",
            ArgumentToken::None_ => "none",
            ArgumentToken::DoubleQuote => "\"",
            ArgumentToken::SingleQuote => "'",
            ArgumentToken::At => "@",
            ArgumentToken::Dot => ".",
            ArgumentToken::TypeArgString => "<...>",
            ArgumentToken::Input => "input",
            ArgumentToken::Result => "result",
            ArgumentToken::Gas => "gas",
        };
        fmt::Display::fmt(s, formatter)
    }
}

impl Token for ArgumentToken {
    fn is_whitespace(&self) -> bool {
        matches!(self, Self::Whitespace)
    }

    fn next_token(s: &str) -> anyhow::Result<Option<(Self, usize)>> {
        // parses a string where start matches end.
        // performs simple matching for start/end pairs
        fn number_maybe_with_suffix(text: &str, num_text_len: usize) -> (ArgumentToken, usize) {
            let rest = &text[num_text_len..];
            if rest.starts_with("u8") {
                (ArgumentToken::NumberTyped, num_text_len + 2)
            } else if rest.starts_with("u64") || rest.starts_with("u16") || rest.starts_with("u32")
            {
                (ArgumentToken::NumberTyped, num_text_len + 3)
            } else if rest.starts_with("u128") || rest.starts_with("u256") {
                (ArgumentToken::NumberTyped, num_text_len + 4)
            } else {
                // No typed suffix
                (ArgumentToken::Number, num_text_len)
            }
        }

        // type arguments get delegated to a different parser
        if s.starts_with('<') {
            let len = parse_sub_token_string(s, "<", ">")?;
            return Ok(Some((Self::TypeArgString, len)));
        }

        if s.starts_with("vector[") {
            let len = "vector".len();
            return Ok(Some((Self::Vector, len)));
        }

        if s.starts_with("some(") {
            let len = "some".len();
            return Ok(Some((Self::Some_, len)));
        }

        if s.starts_with("none") {
            let len = "none".len();
            return Ok(Some((Self::None_, len)));
        }

        // other tokens
        let mut chars = s.chars().peekable();
        let c = match chars.next() {
            None => return Ok(None),
            Some(c) => c,
        };

        Ok(Some(match c {
            '(' => (Self::LParen, 1),
            ')' => (Self::RParen, 1),
            '[' => (Self::LBracket, 1),
            ']' => (Self::RBracket, 1),
            ',' => (Self::Comma, 1),
            ':' if matches!(chars.peek(), Some(':')) => (Self::ColonColon, 2),
            ':' => (Self::Colon, 1),
            '"' => {
                let end_quote_byte_offset = match s[1..].find('"') {
                    Some(o) => o,
                    None => bail!("Unexpected end of string before end quote: {}", s),
                };
                let len = s[..1].len() + end_quote_byte_offset + 1;
                if s[..len].chars().any(|c| c == '\\') {
                    bail!(
                        "Escape characters not yet supported in utf8 string: {}",
                        &s[..len]
                    )
                }
                (Self::DoubleQuote, len)
            }
            '\'' => {
                let end_quote_byte_offset = match s[1..].find('\'') {
                    Some(o) => o,
                    None => bail!("Unexpected end of string before end quote: {}", s),
                };
                let len = s[..1].len() + end_quote_byte_offset + 1;
                if s[..len].chars().any(|c| c == '\\') {
                    bail!(
                        "Escape characters not yet supported in string: {}",
                        &s[..len]
                    )
                }
                (Self::SingleQuote, len)
            }
            '@' => (Self::At, 1),
            '.' => (Self::Dot, 1),
            '0' if matches!(chars.peek(), Some('x')) => {
                chars.next().unwrap();
                match chars.next() {
                    Some(c) if c.is_ascii_hexdigit() => {
                        let len = 3 + chars
                            .take_while(|c| char::is_ascii_hexdigit(c) || *c == '_')
                            .count();
                        number_maybe_with_suffix(s, len)
                    }
                    _ => bail!("unrecognized token: {}", s),
                }
            }
            c if c.is_ascii_whitespace() => {
                // c + remaining
                let len = 1 + chars.take_while(char::is_ascii_whitespace).count();
                (Self::Whitespace, len)
            }
            c if c.is_ascii_digit() => {
                // c + remaining
                let len = 1 + chars
                    .take_while(|c| char::is_ascii_digit(c) || *c == '_')
                    .count();
                number_maybe_with_suffix(s, len)
            }
            c if c.is_ascii_alphabetic() || c == '_' => {
                // c + remaining
                let len = 1 + chars
                    .take_while(|c| identifier::is_valid_identifier_char(*c))
                    .count();
                (Self::Ident, len)
            }
            c => bail!("Unrecognized char: {}'", c),
        }))
    }
}

fn parse_sub_token_string(mut s: &str, start: &str, end: &str) -> anyhow::Result<usize> {
    // the length of the string until the matching end
    let mut len = 0;
    let start_len = start.len();
    let end_len = end.len();
    // the count of number of active start/end pairs
    let mut count = 0i32;
    loop {
        s = if s.is_empty() {
            bail!("Unexpected end of string after '{start}'. Expected matching '{end}'")
        } else if let Some(next) = s.strip_prefix(start) {
            len += start_len;
            // new start
            count += 1;
            next
        } else if let Some(next) = s.strip_prefix(end) {
            len += end_len;
            // an end
            count -= 1;
            if count == 0 {
                // end found
                break;
            }
            next
        } else {
            len += 1;
            &s[1..]
        }
    }
    Ok(len)
}

mod tests {
    use super::ArgumentToken;
    use move_command_line_common::parser::Token;

    #[test]
    fn tokenize_vector() {
        let vecs = vec![
            "vector[1,2,3]",
            "vector[1, 2, 3]",
            "vector[]",
            "vector[1]",
            "vector[1,]",
        ];
        for s in &vecs {
            assert!(dbg!(ArgumentToken::tokenize(s)).is_ok());
        }
    }

    #[test]
    fn tokenize_array() {
        let vecs = vec!["[1,2,3]", "[1, 2, 3]", "[]", "[1]", "[1,]"];
        for s in &vecs {
            assert!(dbg!(ArgumentToken::tokenize(s)).is_ok());
        }
    }

    #[test]
    fn tokenize_num() {
        let vecs = vec![
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
        for s in &vecs {
            assert!(dbg!(ArgumentToken::tokenize(s)).is_ok());
        }
    }

    #[test]
    fn tokenize_address() {
        let vecs = vec![
            "@0x1",
            "@0x1_000",
            "@0x100_000_000",
            "@0x100_000u64",
            "@0x1u8",
            "@0x1_u128",
        ];
        for s in &vecs {
            assert!(dbg!(ArgumentToken::tokenize(s)).is_ok());
        }
    }

    #[test]
    fn tokenize_args() {
        let args = vec![
            "@0x1 1 1u8 1_u128 1_000 100_000_000 100_000u64 1 [1, 2, 3, 4, 5, 6, 7, 8, 9, 10] vector[] vector[1,2,3] vector[1]",
            "some(@0x1) none some(vector[1,2,3])",
        ];
        for s in &args {
            assert!(dbg!(ArgumentToken::tokenize(s)).is_ok());
        }
    }
}

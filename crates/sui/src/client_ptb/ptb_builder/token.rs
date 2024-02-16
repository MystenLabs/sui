// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::bail;
use move_command_line_common::parser;
use move_core_types::identifier;
use std::fmt::{self, Display};

// number :=
//     <digit> (<digit>|"_")*       // Allow _ separators in numbers
//     "0x" <digit> (<digit>|"_")*  // Allow hex-encoded numbers
//
// value :=
//     "true"
//     "false"                                        // bool(s)
//     <number><type_suffix>                          // u8, u16, u32, u64, u128, u256
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
//     <var>                         // input
//     <var> ("." (<digit>|<var>))+  // result access
//     "<arg> <arg>*"                // space separated args
//     "gas"                         // gas coin
//
// ty :=
//     <prim_type> // u64, bool, u32, address, u16 (etc)
//     <addr>::<ident>::<ident> (<ty_arg>)?
//     <var>::<ident>::<ident> (<ty_arg>)?
//
// ty_arg :=
//     "<" (<ty>,)+ ">"
//
// var := [a-zA-Z_][a-zA-Z0-9_-]* // Valid move identifier + '-'
//
// ident := [a-zA-Z_][a-zA-Z0-9_]* // Valid move identifier

// commands:
//

#[derive(Eq, PartialEq, Debug, Clone, Copy)]
pub enum PTBToken {
    // any whitespace
    Whitespace,
    // alpha numeric + '-' + '_'
    Ident,
    // digits
    Number,
    // Digits with a type suffix
    NumberTyped,
    // ::
    ColonColon,
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
    // gas
    Gas,

    // Commands
    CommandTransferObjects,
    CommandSplitCoins,
    CommandMergeCoins,
    CommandMakeMoveVec,
    CommandMoveCall,
    CommandPublish,
    CommandUpgrade,
    CommandAssign,
    CommandWarnShadows,
    CommandPreview,
    CommandSummary,
    CommandPickGasBudget,
    CommandGasBudget,
    CommandFile,

    Void,
    Eof,
}

pub const TRANSFER_OBJECTS: &str = "--transfer-objects";
pub const SPLIT_COINS: &str = "--split-coins";
pub const MERGE_COINS: &str = "--merge-coins";
pub const MAKE_MOVE_VEC: &str = "--make-move-vec";
pub const MOVE_CALL: &str = "--move-call";
pub const PUBLISH: &str = "--publish";
pub const UPGRADE: &str = "--upgrade";
pub const ASSIGN: &str = "--assign";
pub const PREVIEW: &str = "--preview";
pub const WARN_SHADOWS: &str = "--warn-shadows";
pub const PICK_GAS_BUDGET: &str = "--pick-gas-budget";
pub const GAS_BUDGET: &str = "--gas-budget";
pub const FILE: &str = "--file";
pub const SUMMARY: &str = "--summary";

pub const ALL_PUBLIC_COMMAND_TOKENS: &[&str] = &[
    TRANSFER_OBJECTS,
    SPLIT_COINS,
    MERGE_COINS,
    MAKE_MOVE_VEC,
    MOVE_CALL,
    PUBLISH,
    UPGRADE,
    ASSIGN,
    PREVIEW,
    WARN_SHADOWS,
    PICK_GAS_BUDGET,
    GAS_BUDGET,
    SUMMARY,
];

impl PTBToken {
    pub fn is_command_token(&self) -> bool {
        match self {
            PTBToken::CommandTransferObjects
            | PTBToken::CommandSplitCoins
            | PTBToken::CommandMergeCoins
            | PTBToken::CommandMakeMoveVec
            | PTBToken::CommandMoveCall
            | PTBToken::CommandPublish
            | PTBToken::CommandUpgrade
            | PTBToken::CommandAssign
            | PTBToken::CommandPreview
            | PTBToken::CommandWarnShadows
            | PTBToken::CommandPickGasBudget
            | PTBToken::CommandGasBudget
            | PTBToken::CommandFile
            | PTBToken::CommandSummary => true,
            _ => false,
        }
    }
}

impl Display for PTBToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        let s = match *self {
            PTBToken::Whitespace => "[whitespace]",
            PTBToken::Void => "[void]",
            PTBToken::Eof=> "[eof]",
            PTBToken::Ident => "[ident]",
            PTBToken::Number => "[number]",
            PTBToken::NumberTyped => "[number_typed]",
            PTBToken::ColonColon => "::",
            PTBToken::Comma => ",",
            PTBToken::LBracket => "[",
            PTBToken::RBracket => "]",
            PTBToken::LParen => "(",
            PTBToken::RParen => ")",
            PTBToken::Vector => "vector",
            PTBToken::Some_ => "some",
            PTBToken::None_ => "none",
            PTBToken::DoubleQuote => "\"",
            PTBToken::SingleQuote => "'",
            PTBToken::At => "@",
            PTBToken::Dot => ".",
            PTBToken::TypeArgString => "<...>",
            PTBToken::Gas => "gas",
            PTBToken::CommandTransferObjects => TRANSFER_OBJECTS,
            PTBToken::CommandSplitCoins => SPLIT_COINS,
            PTBToken::CommandMergeCoins => MERGE_COINS,
            PTBToken::CommandMakeMoveVec => MAKE_MOVE_VEC,
            PTBToken::CommandMoveCall => MOVE_CALL,
            PTBToken::CommandPublish => PUBLISH,
            PTBToken::CommandUpgrade => UPGRADE,
            PTBToken::CommandAssign => ASSIGN,
            PTBToken::CommandPreview => PREVIEW,
            PTBToken::CommandWarnShadows => WARN_SHADOWS,
            PTBToken::CommandPickGasBudget => PICK_GAS_BUDGET,
            PTBToken::CommandGasBudget => GAS_BUDGET,
            PTBToken::CommandFile => FILE,
            PTBToken::CommandSummary => SUMMARY,
        };
        fmt::Display::fmt(s, formatter)
    }
}

impl parser::Token for PTBToken {
    fn is_whitespace(&self) -> bool {
        matches!(self, Self::Whitespace)
    }

    fn next_token(s: &str) -> anyhow::Result<Option<(Self, usize)>> {
        fn number_maybe_with_suffix(text: &str, num_text_len: usize) -> (PTBToken, usize) {
            let rest = &text[num_text_len..];
            if rest.starts_with("u8") {
                (PTBToken::NumberTyped, num_text_len + 2)
            } else if rest.starts_with("u64") || rest.starts_with("u16") || rest.starts_with("u32")
            {
                (PTBToken::NumberTyped, num_text_len + 3)
            } else if rest.starts_with("u128") || rest.starts_with("u256") {
                (PTBToken::NumberTyped, num_text_len + 4)
            } else {
                // No typed suffix
                (PTBToken::Number, num_text_len)
            }
        }

        let non_alphabet_continuation_of = |prefix: &str| -> bool {
            s.starts_with(prefix)
                && s.chars()
                    .nth(prefix.len())
                    .map(|c| !identifier::is_valid_identifier_char(c) && c != '-')
                    .unwrap_or(true)
        };

        let keywords = vec![
            ("vector", Self::Vector),
            ("some", Self::Some_),
            ("none", Self::None_),
            ("gas", Self::Gas),
            (TRANSFER_OBJECTS, Self::CommandTransferObjects),
            (SPLIT_COINS, Self::CommandSplitCoins),
            (MERGE_COINS, Self::CommandMergeCoins),
            (MAKE_MOVE_VEC, Self::CommandMakeMoveVec),
            (MOVE_CALL, Self::CommandMoveCall),
            (PUBLISH, Self::CommandPublish),
            (UPGRADE, Self::CommandUpgrade),
            (ASSIGN, Self::CommandAssign),
            (PREVIEW, Self::CommandPreview),
            (WARN_SHADOWS, Self::CommandWarnShadows),
            (PICK_GAS_BUDGET, Self::CommandPickGasBudget),
            (GAS_BUDGET, Self::CommandGasBudget),
            (SUMMARY, Self::CommandSummary),
            (FILE, Self::CommandFile),
        ];

        // type arguments get delegated to a different parser
        if s.starts_with('<') {
            let len = extract_sub_parser_token_string(s, "<", ">")?;
            return Ok(Some((Self::TypeArgString, len)));
        }

        for keyword in keywords {
            if non_alphabet_continuation_of(keyword.0) {
                let len = keyword.0.len();
                return Ok(Some((keyword.1, len)));
            }
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
            '"' => {
                let end_quote_byte_offset = match s[1..].find('"') {
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
                    .take_while(|c| identifier::is_valid_identifier_char(*c) || *c == '-')
                    .count();
                (Self::Ident, len)
            }
            c => bail!("Unrecognized character: {}'", c),
        }))
    }
}

fn extract_sub_parser_token_string(mut s: &str, start: &str, end: &str) -> anyhow::Result<usize> {
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

#[cfg(test)]
mod tests {
    use move_command_line_common::parser::Token;

    use crate::client_ptb::ptb_builder::token::PTBToken;

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
            assert!(PTBToken::tokenize(s).is_ok());
        }
    }

    #[test]
    fn tokenize_array() {
        let vecs = vec!["[1,2,3]", "[1, 2, 3]", "[]", "[1]", "[1,]"];
        for s in &vecs {
            assert!(PTBToken::tokenize(s).is_ok());
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
            assert!(PTBToken::tokenize(s).is_ok());
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
            assert!(PTBToken::tokenize(s).is_ok());
        }
    }

    #[test]
    fn tokenize_args() {
        let args = vec![
            "@0x1 1 1u8 1_u128 1_000 100_000_000 100_000u64 1 [1, 2, 3, 4, 5, 6, 7, 8, 9, 10] vector[] vector[1,2,3] vector[1]",
            "some(@0x1) none some(vector[1,2,3]) --assign --transfer-objects --split-coins --merge-coins --make-move-vec --move-call --publish --upgrade --preview --warn-shadows --pick-gas-budget --gas-budget --summary",
        ];
        for s in &args {
            assert!(PTBToken::tokenize(s).is_ok());
        }
    }

    #[test]
    fn dotted_idents() {
        let args = vec!["a", "a.b", "a.b.c", "a.b.c.d", "a.b.c.d.e"];
        for s in &args {
            assert!(PTBToken::tokenize(s).is_ok());
        }
    }

    #[test]
    fn gas() {
        let args = vec!["gas"];
        for s in &args {
            assert!(PTBToken::tokenize(s).is_ok());
        }
    }
}


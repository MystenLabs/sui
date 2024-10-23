// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt::{self, Display};

use anyhow::bail;
use move_core_types::identifier;
use move_core_types::parsing::parser::Token;

#[derive(Eq, PartialEq, Debug, Clone, Copy)]
pub enum CommandToken {
    // any whitespace
    Whitespace,
    // // or /* */
    Comment,
    // //>
    CommandStart,
    // alpha numeric
    Ident,
    // digits
    Number,
    // ::
    ColonColon,
    // :
    Colon,
    // ,
    Comma,
    // ;
    Semi,
    // [
    LBracket,
    // ]
    RBracket,
    // (
    LParen,
    // )
    RParen,
    // <...>
    // eats the whole string, including the < and >, to pass to a different parser
    TypeArgString,
    // uninhabited token
    Void,
}

pub const TRANSFER_OBJECTS: &str = "TransferObjects";
pub const SPLIT_COINS: &str = "SplitCoins";
pub const MERGE_COINS: &str = "MergeCoins";
pub const MAKE_MOVE_VEC: &str = "MakeMoveVec";
pub const PUBLISH: &str = "Publish";
pub const UPGRADE: &str = "Upgrade";
pub const GAS_COIN: &str = "Gas";
pub const INPUT: &str = "Input";
pub const RESULT: &str = "Result";
pub const NESTED_RESULT: &str = "NestedResult";

impl Display for CommandToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        let s = match *self {
            CommandToken::Whitespace => "[whitespace]",
            CommandToken::Comment => "[comment]",
            CommandToken::Ident => "[identifier]",
            CommandToken::Number => "[num]",
            CommandToken::CommandStart => "//>",
            CommandToken::ColonColon => "::",
            CommandToken::Colon => ":",
            CommandToken::Comma => ",",
            CommandToken::Semi => ";",
            CommandToken::LBracket => "[",
            CommandToken::RBracket => "]",
            CommandToken::LParen => "(",
            CommandToken::RParen => ")",
            CommandToken::TypeArgString => "<...>",
            CommandToken::Void => "[void]",
        };
        fmt::Display::fmt(s, formatter)
    }
}

impl Token for CommandToken {
    fn is_whitespace(&self) -> bool {
        matches!(self, Self::Whitespace | Self::Comment | Self::Void)
    }

    fn next_token(s: &str) -> anyhow::Result<Option<(Self, usize)>> {
        // parses a string where start matches end.
        // performs simple matching for start/end pairs

        // type arguments get delegated to a different parser
        if s.starts_with('<') {
            let len = parse_sub_token_string(s, "<", ">")?;
            return Ok(Some((Self::TypeArgString, len)));
        }
        // start of a command
        if s.starts_with("//>") {
            return Ok(Some((Self::CommandStart, 3)));
        }
        // comments
        if let Some(after) = s.strip_prefix("//") {
            let mut n = 2;
            let mut in_whitespace_from_start = true;
            for c in after.chars() {
                n += 1;
                if c == '\n' {
                    break;
                }
                if in_whitespace_from_start && c == '>' {
                    bail!("Remove whitespace between // and > to start a command");
                }
                if !c.is_whitespace() {
                    in_whitespace_from_start = false;
                }
            }
            return Ok(Some((Self::Comment, n)));
        }
        if s.starts_with("/*") {
            let end = parse_sub_token_string(s, "/*", "*/")?;
            return Ok(Some((Self::Comment, end)));
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
            ';' => (Self::Semi, 1),
            ':' if matches!(chars.peek(), Some(':')) => (Self::ColonColon, 2),
            ':' => (Self::Colon, 1),
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
                (CommandToken::Number, len)
            }
            c if c.is_ascii_alphabetic() || c == '_' => {
                // c + remaining
                let len = 1 + chars
                    .take_while(|c| identifier::is_valid_identifier_char(*c))
                    .count();
                (Self::Ident, len)
            }
            _ => bail!("unrecognized token: {}", s),
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

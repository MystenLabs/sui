// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt;

#[derive(Clone, Copy, Debug)]
pub struct Lexeme<'t> {
    /// The kind of lexeme.
    pub token: Token,
    /// Slice from source that identifies this lexeme (among other instances of this token).
    pub src: &'t str,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Token {
    /// --[a-zA-Z0-9_-]+
    Command,
    /// [a-zA-Z_][a-zA-Z0-9_-]*
    Ident,
    /// [1-9][0-9_]*
    Number,
    /// 0x[0-9a-fA-F][0-9a-fA-F_]*
    HexNumber,
    /// "..." | '...'
    String,
    /// ::
    ColonColon,
    /// ,
    Comma,
    /// [
    LBracket,
    /// ]
    RBracket,
    /// (
    LParen,
    /// )
    RParen,
    /// <
    LAngle,
    /// >
    RAngle,
    /// @
    At,
    /// .
    Dot,

    /// Special tokens for unexpected lexer states that the parser should error on.
    Unexpected,
    UnfinishedString,
    EarlyEof,

    // The following tokens are special -- they consume multiple shell tokens, to ensure we capture
    // the path for a publish or an upgrade command.
    /// --publish <shell-token>
    Publish,
    /// --upgraded <shell-token>
    Upgrade,
}

impl<'a> fmt::Display for Lexeme<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use Token as T;

        match self.token {
            T::Command => write!(f, "command '--{}'", self.src),
            T::Ident => write!(f, "identifier '{}'", self.src),
            T::Number => write!(f, "number '{}'", self.src),
            T::HexNumber => write!(f, "hexadecimal number '0x{}'", self.src),
            T::String => write!(f, "string {:?}", self.src),
            T::ColonColon => write!(f, "'::'"),
            T::Comma => write!(f, "','"),
            T::LBracket => write!(f, "'['"),
            T::RBracket => write!(f, "']'"),
            T::LParen => write!(f, "'('"),
            T::RParen => write!(f, "')'"),
            T::LAngle => write!(f, "'<'"),
            T::RAngle => write!(f, "'>'"),
            T::At => write!(f, "'@'"),
            T::Dot => write!(f, "'.'"),
            T::Unexpected => write!(f, "unexpected input {:?}", self.src),
            T::UnfinishedString => write!(f, "unfinished string {:?}", format!("{}...", self.src)),
            T::EarlyEof => write!(f, "unexpected end of file"),
            T::Publish => write!(f, "command '--publish {:?}'", self.src),
            T::Upgrade => write!(f, "command '--upgrade {:?}'", self.src),
        }
    }
}

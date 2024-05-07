// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt;

#[derive(Clone, Copy, Debug)]
pub struct Lexeme<'t>(
    /// The kind of lexeme.
    pub Token,
    /// Slice from source that identifies this lexeme (among other instances of this token).
    pub &'t str,
);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Token {
    /// --[a-zA-Z0-9_-]+
    Command,
    /// -[a-zA-Z0-9]
    Flag,
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

    /// End of input.
    Eof,

    /// Special tokens for unexpected lexer states that the parser should error on.
    Unexpected,
    UnfinishedString,
    EarlyEof,

    // The following tokens are special -- they consume multiple shell tokens, to ensure we capture
    // the path for a publish or an upgrade command.
    /// --publish \<shell-token\>
    Publish,
    /// --upgraded \<shell-token\>
    Upgrade,
}

impl<'l> Lexeme<'l> {
    /// Returns true if this lexeme corresponds to a special error token.
    pub fn is_error(&self) -> bool {
        use Token as T;
        matches!(self.0, T::Unexpected | T::UnfinishedString | T::EarlyEof)
    }

    /// Returns true if this is the kind of lexeme that finishes the token stream.
    pub fn is_terminal(&self) -> bool {
        self.is_error() || self.0 == Token::Eof
    }

    /// Returns true if this lexeme signifies the end of the current command.
    pub fn is_command_end(&self) -> bool {
        self.is_terminal() || [Token::Command, Token::Publish, Token::Upgrade].contains(&self.0)
    }
}

impl<'a> fmt::Display for Lexeme<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use Token as T;

        match self.0 {
            T::Command => write!(f, "command '--{}'", self.1),
            T::Flag => write!(f, "flag '-{}'", self.1),
            T::Ident => write!(f, "identifier '{}'", self.1),
            T::Number => write!(f, "number '{}'", self.1),
            T::HexNumber => write!(f, "hexadecimal number '0x{}'", self.1),
            T::String => write!(f, "string {:?}", self.1),
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
            T::Unexpected => write!(f, "input {:?}", self.1),
            T::UnfinishedString => write!(f, "unfinished string {:?}", format!("{}...", self.1)),
            T::EarlyEof | T::Eof => write!(f, "end of input"),
            T::Publish => write!(f, "command '--publish {:?}'", self.1),
            T::Upgrade => write!(f, "command '--upgrade {:?}'", self.1),
        }
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use Token as T;
        match self {
            T::Command => write!(f, "a command"),
            T::Flag => write!(f, "a flag"),
            T::Ident => write!(f, "an identifier"),
            T::Number => write!(f, "a number"),
            T::HexNumber => write!(f, "a hexadecimal number"),
            T::String => write!(f, "a string"),
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
            T::Eof => write!(f, "end of input"),
            T::Unexpected => write!(f, "unexpected input"),
            T::UnfinishedString => write!(f, "an unfinished string"),
            T::EarlyEof => write!(f, "unexpected end of input"),
            T::Publish => write!(f, "a '--publish' command"),
            T::Upgrade => write!(f, "an '--upgrade' command"),
        }
    }
}

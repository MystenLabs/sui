// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeSet, fmt};

use super::lexer::{OwnedLexeme, Token};
use super::peek::Peekable2Ext;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Number literal is too large to fit into '{what}'")]
    NumberOverflow { what: &'static str },

    #[error("Unexpected end-of-string, expected {expect}")]
    UnexpectedEos { expect: ExpectedSet },

    #[error("Unexpected {actual}, expected {expect}")]
    UnexpectedToken {
        actual: OwnedLexeme,
        expect: ExpectedSet,
    },
}

/// The set of patterns that the parser tried to match against the next token, in a given
/// invocation of `match_token!` or `match_token_opt!`. This is used to provide a clearer error
/// message.
#[derive(Debug)]
pub(crate) struct ExpectedSet {
    /// Sets form a linked list, where each set value corresponds to a single `match_token!` or
    /// `match_token_opt!` invocation. Each invocation optionally accepts the set of previously
    /// attempted patterns.
    pub prev: Option<Box<ExpectedSet>>,

    /// The set of patterns that were tried in this invocation.
    pub tried: &'static [Expected],
}

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum Expected {
    /// Expected a token spanning a particular literal slice.
    Literal(&'static str),
    /// Expected any slice of source string that matches a specific token.
    Token(Token),
}

/// The result of a `match_token_opt!` invocation, which can succeed, or return the set of patterns
/// it tried.
pub(crate) enum Match<T> {
    Found(T),
    Tried(ExpectedSet),
}

impl fmt::Display for Expected {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Expected::Token(token) => write!(f, "{token}"),
            Expected::Literal(s) => write!(f, "'{s}'"),
        }
    }
}

impl fmt::Display for ExpectedSet {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // Gather all the tokens that were tried.
        let mut expected = BTreeSet::new();
        let mut curr = Some(self);
        while let Some(set) = curr {
            expected.extend(set.tried);
            curr = set.prev.as_ref().map(Box::as_ref)
        }

        if expected.is_empty() {
            return write!(f, "nothing");
        }

        let mut tokens = expected.into_iter().peekable2();
        let mut prefix = if tokens.peek2().is_some() {
            "one of "
        } else {
            ""
        };

        while let Some(token) = tokens.next() {
            write!(f, "{prefix}{token}")?;
            prefix = if tokens.peek2().is_some() {
                ", "
            } else {
                ", or "
            };
        }

        Ok(())
    }
}

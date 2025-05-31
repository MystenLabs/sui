// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::mem;
use std::{collections::BTreeSet, fmt};

use super::lexer::{Lexeme, OwnedLexeme, Token};
use super::peek::Peekable2Ext;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Hex {0} contains invalid character")]
    InvalidHexCharacter(OwnedLexeme),

    #[error("Invalid {0}")]
    InvalidIdentifier(OwnedLexeme),

    #[error("Number literal is too large to fit into {what}")]
    NumberOverflow { what: &'static str },

    #[error("Odd number of characters in hex {0}")]
    OddHexLiteral(OwnedLexeme),

    #[error("Unexpected end-of-string, expected {expect}")]
    UnexpectedEos { expect: ExpectedSet },

    #[error("Unexpected {actual}, expected {expect}")]
    UnexpectedToken {
        actual: OwnedLexeme,
        expect: ExpectedSet,
    },

    #[error("vector at offset {offset} requires 1 type parameter, found {arity}")]
    VectorArity { offset: usize, arity: usize },
}

/// The set of patterns that the parser tried to match against the next token, in a given
/// invocation of `match_token!` or `match_token_opt!`. This is used to provide a clearer error
/// message.
#[derive(Debug, Clone)]
pub(crate) struct ExpectedSet {
    /// Other sets of patterns that were attempted on the same location.
    pub prev: Vec<ExpectedSet>,

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
/// it tried, and the offset they were tried at.
#[derive(Clone)]
pub(crate) enum Match<T> {
    Found(T),
    Tried(Option<usize>, ExpectedSet),
}

impl Error {
    // Indicate that `tried` was also tried at `offset`, in case the error is related to other
    // tokens that were tried at the same location.
    pub(crate) fn also_tried(self, offset: Option<usize>, tried: ExpectedSet) -> Self {
        match (offset, self) {
            (Some(offset), Error::UnexpectedToken { actual, expect }) if offset == actual.1 => {
                Error::UnexpectedToken {
                    actual,
                    expect: expect.union(tried),
                }
            }

            (None, Error::UnexpectedEos { expect }) => Error::UnexpectedEos {
                expect: expect.union(tried),
            },

            (_, error) => error,
        }
    }
}

impl ExpectedSet {
    pub(crate) fn new(tried: &'static [Expected]) -> Self {
        Self {
            prev: vec![],
            tried,
        }
    }

    pub(crate) fn with_prev(mut self, prev: ExpectedSet) -> Self {
        self.prev.push(prev);
        self
    }

    pub(crate) fn union(mut self, mut other: ExpectedSet) -> Self {
        // Always arrange for `self` to be the larger set, so that we drain the smaller set into
        // the larger, to always do the minimal work. Ordering does not matter because tokens
        // across all sets are collected into a set before displaying them.
        if self.prev.len() < other.prev.len() {
            mem::swap(&mut self, &mut other);
        }

        self.prev.append(&mut other.prev);
        self.prev.push(other);
        self
    }

    pub(crate) fn into_error(self, actual: Option<&Lexeme<'_>>) -> Error {
        if let Some(actual) = actual {
            Error::UnexpectedToken {
                actual: actual.detach(),
                expect: self,
            }
        } else {
            Error::UnexpectedEos { expect: self }
        }
    }
}

impl<T> Match<T> {
    pub(crate) fn is_not_found(&self) -> bool {
        matches!(self, Match::Tried(_, _))
    }
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
        let mut stack = vec![self];
        while let Some(set) = stack.pop() {
            expected.extend(set.tried);
            stack.extend(&set.prev);
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

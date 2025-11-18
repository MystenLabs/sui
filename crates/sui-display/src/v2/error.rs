// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;
use std::fmt;
use std::mem;
use std::sync::Arc;

use move_core_types::annotated_visitor;
use move_core_types::language_storage::TypeTag;

use crate::v2::lexer::Lexeme;
use crate::v2::lexer::OwnedLexeme;
use crate::v2::lexer::Token;
use crate::v2::peek::Peekable2Ext;

/// Errors related to the display format as a whole.
///
/// NB. Limit errors (`Too*`) are duplicated here and in `FormatError` because they occur while
/// working on a format, and need to be propagated up to the Display overall.
#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Duplicate name {0:?}")]
    NameDuplicate(String),

    #[error("Name pattern {0:?} produced no output")]
    NameEmpty(String),

    #[error("Name pattern {0:?} did not evaluate to a string")]
    NameInvalid(String),

    #[error("Error evaluating name pattern {0:?}: {1}")]
    NameError(String, FormatError),

    #[error("Display contains too many elements")]
    TooBig,

    #[error("Display tries to load too many objects")]
    TooManyLoads,

    #[error("Display produces too much output")]
    TooMuchOutput,
}

/// Errors related to a single format string.
#[derive(thiserror::Error, Debug, Clone)]
pub enum FormatError {
    #[error("BCS error: {0}")]
    Bcs(#[from] bcs::Error),

    #[error("Hex {0} contains invalid character")]
    InvalidHexCharacter(OwnedLexeme),

    #[error("Invalid {0}")]
    InvalidIdentifier(OwnedLexeme),

    #[error("Invalid {what}: {err}")]
    InvalidNumber { what: &'static str, err: String },

    #[error("Odd number of characters in hex {0}")]
    OddHexLiteral(OwnedLexeme),

    #[error("Storage error: {0}")]
    Store(Arc<anyhow::Error>),

    #[error("Display contains too many elements")]
    TooBig,

    #[error("Format is nested too deeply")]
    TooDeep,

    #[error("Display tries to load too many objects")]
    TooManyLoads,

    #[error("Display produces too much output")]
    TooMuchOutput,

    #[error("Invalid transform: {0}")]
    TransformInvalid(&'static str),

    #[error("Unexpected end-of-string, expected {expect}")]
    UnexpectedEos { expect: ExpectedSet },

    #[error("Unexpected {actual}, expected {expect}")]
    UnexpectedToken {
        actual: OwnedLexeme,
        expect: ExpectedSet,
    },

    #[error("Vector at offset {offset} requires 1 type parameter, found {arity}")]
    VectorArity { offset: usize, arity: usize },

    #[error("Internal error: vector without element type")]
    VectorNoType,

    #[error(
        "Vector literal's element type, could be {} or {}",
        .0.to_canonical_display(true),
        .1.to_canonical_display(true),
    )]
    VectorTypeMismatch(TypeTag, TypeTag),

    #[error("Deserialization error: {0}")]
    Visitor(#[from] annotated_visitor::Error),
}

/// The set of patterns that the parser tried to match against the next token, in a given
/// invocation of `match_token!` or `match_token_opt!`. This is used to provide a clearer error
/// message.
#[derive(Debug, Clone)]
pub struct ExpectedSet {
    /// Other sets of patterns that were attempted on the same location.
    prev: Vec<ExpectedSet>,

    /// The set of patterns that were tried in this invocation.
    tried: &'static [Expected],
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

impl FormatError {
    // Indicate that `tried` was also tried at `offset`, in case the error is related to other
    // tokens that were tried at the same location.
    pub(crate) fn also_tried(self, offset: Option<usize>, tried: ExpectedSet) -> Self {
        match (offset, self) {
            (Some(offset), FormatError::UnexpectedToken { actual, expect })
                if offset == actual.2 =>
            {
                FormatError::UnexpectedToken {
                    actual,
                    expect: expect.union(tried),
                }
            }

            (None, FormatError::UnexpectedEos { expect }) => FormatError::UnexpectedEos {
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

    pub(crate) fn into_error(self, actual: Option<&Lexeme<'_>>) -> FormatError {
        if let Some(actual) = actual {
            FormatError::UnexpectedToken {
                actual: actual.detach(),
                expect: self,
            }
        } else {
            FormatError::UnexpectedEos { expect: self }
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

impl From<std::fmt::Error> for FormatError {
    fn from(_: std::fmt::Error) -> Self {
        FormatError::TooMuchOutput
    }
}

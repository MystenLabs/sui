// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt;

/// Represents the location of a range of text in the PTB source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

/// A value that has an associated location in source code.
pub struct Spanned<T> {
    pub span: Span,
    pub value: T,
}

#[macro_export]
macro_rules! sp_ {
    (_, $value:pat) => {
        $crate::client_ptb::error::Spanned { value: $value, .. }
    };
    ($loc:pat, _) => {
        $crate::client_ptb::error::Spanned { span: $loc, .. }
    };
    ($loc:pat, $value:pat) => {
        $crate::client_ptb::error::Spanned {
            span: $loc,
            value: $value,
        }
    };
}

pub use sp_;

impl<T> Spanned<T> {
    /// Apply a function `f` to the underlying value, returning a new `Spanned` with the same span.
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Spanned<U> {
        Spanned {
            span: self.span,
            value: f(self.value),
        }
    }

    /// TODO Docs
    pub fn widen<U>(self, other: Spanned<U>) -> Spanned<T> {
        Spanned {
            span: Span {
                start: self.span.start.min(other.span.start),
                end: self.span.end.max(other.span.end),
            },
            value: self.value,
        }
    }
}

impl<T: fmt::Debug> fmt::Debug for Spanned<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Spanned")
            .field("span", &self.span)
            .field("value", &self.value)
            .finish()
    }
}

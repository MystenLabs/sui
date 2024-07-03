// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use miette::{miette, LabeledSpan, Severity};
use std::fmt;
use thiserror::Error;

pub type PTBResult<T> = Result<T, PTBError>;

/// Represents the location of a range of text in the PTB source.
#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

/// A value that has an associated location in source code.
pub struct Spanned<T> {
    pub span: Span,
    pub value: T,
}

/// An error with a message, a location in the source code, and an optional help message.
#[derive(Debug, Clone, Error)]
#[error("{message}")]
pub struct PTBError {
    pub message: String,
    pub span: Span,
    pub help: Option<String>,
    pub severity: Severity,
}

#[macro_export]
macro_rules! sp {
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

#[macro_export]
macro_rules! error {
    ($l:expr, $($arg:tt)*) => {
        return Err($crate::err!($l, $($arg)*))
    };
    ($l:expr => help: { $($h:expr),* }, $($arg:tt)*) => {
        return Err($crate::err!($l => help: { $($h),* }, $($arg)*))
    };
}

#[macro_export]
macro_rules! err {
    ($l:expr, $($arg:tt)*) => {
        $crate::client_ptb::error::PTBError {
            message: format!($($arg)*),
            span: $l,
            help: None,
            severity: miette::Severity::Error,
        }
    };
    ($l:expr => help: { $($h:expr),* }, $($arg:tt)*) => {
        $crate::client_ptb::error::PTBError {
            message: format!($($arg)*),
            span: $l,
            help: Some(format!($($h),*)),
            severity: miette::Severity::Error,
        }
    };
}

pub use sp;

impl PTBError {
    /// Add a help message to an error.
    pub fn with_help(self, help: String) -> Self {
        let PTBError {
            message,
            span,
            help: _,
            severity,
        } = self;
        PTBError {
            message,
            span,
            help: Some(help),
            severity,
        }
    }
}

impl Span {
    /// Wrap a value with a span.
    pub fn wrap<T>(self, value: T) -> Spanned<T> {
        Spanned { span: self, value }
    }

    /// Widen the span to include another span. The resulting span will start at the minimum of the
    /// two start positions and end at the maximum of the two end positions.
    pub fn widen(self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }

    /// Widen the span to include another if it is Some, otherwise return the original span.
    pub fn widen_opt(self, other: Option<Span>) -> Span {
        other.map_or(self, |other| self.widen(other))
    }

    /// Create a span that points to the end of the file/string contents.
    pub fn eof_span() -> Span {
        Self {
            start: usize::MAX,
            end: usize::MAX,
        }
    }
}

impl<T> Spanned<T> {
    /// Apply a function `f` to the underlying value, returning a new `Spanned` with the same span.
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Spanned<U> {
        Spanned {
            span: self.span,
            value: f(self.value),
        }
    }

    /// Widen the span to include another span. The resulting span will start at the minimum of the
    /// two start positions and end at the maximum of the two end positions.
    pub fn widen<U>(self, other: Spanned<U>) -> Spanned<T> {
        self.widen_span(other.span)
    }

    /// Widen the span to include another span. The resulting span will start at the minimum of the
    /// two start positions and end at the maximum of the two end positions.
    pub fn widen_span(self, other: Span) -> Spanned<T> {
        Spanned {
            span: self.span.widen(other),
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

impl<T: Clone> Clone for Spanned<T> {
    fn clone(&self) -> Self {
        Spanned {
            span: self.span,
            value: self.value.clone(),
        }
    }
}

impl<T: Copy> Copy for Spanned<T> {}

fn build_error_report(file_string: &str, error: PTBError) -> miette::Report {
    let PTBError {
        span,
        message,
        help,
        severity,
    } = error;
    let clamp = |x: usize| x.min(file_string.len() - 1);
    let label = LabeledSpan::at(clamp(span.start)..clamp(span.end), message.clone());
    let error_string = match severity {
        Severity::Advice => "Advice found when processing PTB".to_string(),
        Severity::Warning => "Warning when processing PTB".to_string(),
        Severity::Error => "Error when processing PTB".to_string(),
    };
    match help {
        Some(help_msg) => miette!(labels = vec![label], help = help_msg, "{}", error_string),
        None => miette!(
            labels = vec![label],
            severity = severity,
            "{}",
            error_string
        ),
    }
    .with_source_code(file_string.to_string())
}

pub fn build_error_reports(source_string: &str, errors: Vec<PTBError>) -> Vec<miette::Report> {
    errors
        .into_iter()
        .map(|e| build_error_report(source_string, e))
        .collect()
}

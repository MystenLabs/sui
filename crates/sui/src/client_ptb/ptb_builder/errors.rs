// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::client_ptb::ptb_builder::{context::FileScope, utils::to_ordinal_contraction};
use miette::{miette, LabeledSpan};
use move_symbol_pool::Symbol;
use std::{collections::BTreeMap, fmt::Debug};
use thiserror::Error;

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
        $crate::client_ptb::ptb_builder::errors::PTBError::WithSource {
            message: format!($($arg)*),
            span: $l,
            help: None,
        }
    };
    ($l:expr => help: { $($h:expr),* }, $($arg:tt)*) => {
        $crate::client_ptb::ptb_builder::errors::PTBError::WithSource {
            message: format!($($arg)*),
            span: $l,
            help: Some(format!($($h),*)),
        }
    };
}

#[macro_export]
macro_rules! sp {
    (_, $value:pat) => {
        $crate::client_ptb::ptb_builder::errors::Spanned { value: $value, .. }
    };
    ($loc:pat, _) => {
        $crate::client_ptb::ptb_builder::errors::Spanned { span: $loc, .. }
    };
    ($loc:pat, $value:pat) => {
        $crate::client_ptb::ptb_builder::errors::Spanned {
            span: $loc,
            value: $value,
        }
    };
}

#[macro_export]
macro_rules! bind {
    ($loc:pat, $value:pat = $rhs:expr, $err:expr) => {
        let x = $rhs;
        let loc = x.span;
        let ($loc, $value) = (loc.clone(), x.value) else {
            return $err(loc);
        };
    };
}

pub type PTBResult<T> = Result<T, PTBError>;
pub type FileTable = BTreeMap<Symbol, String>;

/// An error that occurred while working with a PTB. This error contains an error message along
/// with the file scope (file name and command index) where the error occurred.
#[derive(Debug, Clone, Error)]
pub enum PTBError {
    #[error("{message} at command {} in file '{}'", span.file_scope.file_command_index, span.file_scope.name)]
    WithSource {
        message: String,
        span: Span,
        help: Option<String>,
    },
}

/// Represents a span of a command in a PTB file. The span is represented as a character range
/// into the given file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub file_scope: FileScope,
}

/// Represents a value in a PTB file along with its span.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spanned<T: Debug + Clone + PartialEq + Eq> {
    pub span: Span,
    pub value: T,
}

impl<T: Debug + Clone + PartialEq + Eq> Spanned<T> {
    pub fn map<U: Debug + Clone + PartialEq + Eq, F: FnOnce(T) -> U>(self, f: F) -> Spanned<U> {
        let Spanned { span, value } = self;
        Spanned {
            span,
            value: f(value),
        }
    }
}

impl PTBError {
    /// Add a help message to an error.
    pub fn with_help(self, help: String) -> Self {
        match self {
            PTBError::WithSource { message, span, .. } => PTBError::WithSource {
                message,
                span,
                help: Some(help),
            },
        }
    }
}

impl Span {
    pub fn new(start: usize, end: usize, file_scope: FileScope) -> Self {
        Self {
            start,
            end,
            file_scope,
        }
    }

    /// Return the union of this span with a set of other spans.
    pub fn union_with(&self, others: impl IntoIterator<Item = Span>) -> Span {
        let mut start = self.start;
        let mut end = self.end;

        for s in others {
            start = start.min(s.start);
            end = end.max(s.end);
        }

        Span {
            start,
            end,
            file_scope: self.file_scope,
        }
    }

    /// Union a (possibly empty) set of spans into a single span. If the set of spans is empty,
    /// `None` is returned.
    pub fn union_spans(others: impl IntoIterator<Item = Span>) -> Option<Span> {
        let mut iter = others.into_iter();
        iter.next().map(|first| first.union_with(iter))
    }

    /// A span representing the command string of whichever file scope this span is in.
    pub fn cmd_span(cmd_len: usize, file_scope: FileScope) -> Span {
        Span {
            start: 0,
            end: cmd_len,
            file_scope,
        }
    }

    /// Create a special span to represent an out-of-band error. These are errors that arise that
    /// cannot be directly attributed to a specific command in the PTB file. E.g., failing to set a
    /// gas budget.
    pub fn out_of_band_span() -> Span {
        Span {
            start: usize::MAX,
            end: usize::MAX,
            file_scope: FileScope {
                file_command_index: 0,
                name: Symbol::from("console"),
                name_index: 0,
            },
        }
    }

    pub fn is_out_of_band_span(&self) -> bool {
        self.start == usize::MAX
    }
}

pub fn span<T: Debug + Clone + PartialEq + Eq>(loc: Span, value: T) -> Spanned<T> {
    Spanned { span: loc, value }
}

struct DisplayableError<'a> {
    file_string: &'a str,
    label: LabeledSpan,
    error_string: String,
    help: Option<String>,
}

impl<'a> DisplayableError<'a> {
    // If no span we point to the command name
    // If there is a span, we convert the span range to the appropriate offset in the whole string for
    // the command
    fn new(file_string: &'a str, error: PTBError) -> Self {
        let PTBError::WithSource {
            span,
            message,
            help,
        } = error;
        let label = LabeledSpan::at(span.start..span.end, message.clone());
        let file_scope = span.file_scope;
        let error_string = format!(
            "{} {}",
            file_scope.file_command_index,
            if &*file_scope.name == "console" {
                "from console input".to_string()
            } else {
                let usage_string = if file_scope.name_index == 0 {
                    "".to_string()
                } else {
                    format!(
                        "({} usage of the file)",
                        to_ordinal_contraction(file_scope.name_index + 1)
                    )
                };
                format!("in PTB file '{}' {usage_string}", file_scope.name)
            }
        );
        Self {
            file_string,
            label,
            error_string,
            help,
        }
    }

    fn create_report(self) -> miette::Report {
        match self.help {
            Some(help_msg) => miette!(
                labels = vec![self.label],
                help = help_msg,
                "Error at command {}",
                self.error_string
            ),
            None => miette!(
                labels = vec![self.label],
                "Error at command {}",
                self.error_string
            ),
        }
        .with_source_code(self.file_string.to_string())
    }
}

pub fn render_errors(
    file_table: &BTreeMap<Symbol, String>,
    errors: Vec<PTBError>,
) -> Vec<miette::Report> {
    let mut rendered = vec![];
    for error in errors {
        let PTBError::WithSource { span, .. } = &error;
        let file_string_opt = file_table.get(&span.file_scope.name);
        if span.is_out_of_band_span() || file_string_opt.is_none() {
            rendered.push(miette!(
                labels = vec![],
                "Error at command {} in PTB file '{}': {}",
                span.file_scope.file_command_index,
                span.file_scope.name,
                error
            ));
        } else if let Some(file_string) = file_string_opt {
            rendered.push(DisplayableError::new(file_string, error).create_report());
        }
    }
    rendered
}

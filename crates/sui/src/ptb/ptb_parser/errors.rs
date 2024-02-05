// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fmt::Debug;

use miette::{miette, LabeledSpan};
use move_symbol_pool::Symbol;
use thiserror::Error;

use crate::ptb::{ptb::PTBCommand, ptb_parser::utils::to_ordinal_contraction};

use super::{
    command_token::{FILE_END, FILE_START},
    context::FileScope,
};

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
        $crate::ptb::ptb_parser::errors::PTBError::WithSource {
            message: format!($($arg)*),
            span: $l,
            help: None,
        }
    };
    ($l:expr => help: { $($h:expr),* }, $($arg:tt)*) => {
        $crate::ptb::ptb_parser::errors::PTBError::WithSource {
            message: format!($($arg)*),
            span: $l,
            help: Some(format!($($h),*)),
        }
    };
}

#[macro_export]
macro_rules! sp {
    (_, $value:pat) => {
        $crate::ptb::ptb_parser::errors::Spanned { value: $value, .. }
    };
    ($loc:pat, _) => {
        $crate::ptb::ptb_parser::errors::Spanned { span: $loc, .. }
    };
    ($loc:pat, $value:pat) => {
        $crate::ptb::ptb_parser::errors::Spanned {
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
/// where the `arg_idx` field indicates which argument the span is within.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub arg_idx: usize,
    pub file_scope: FileScope,
}

/// Represents a value in a PTB file along with its span.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spanned<T: Debug + Clone + PartialEq + Eq> {
    pub span: Span,
    pub value: T,
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

/// A map from file scope to a list of commands in that file scope. Note that these commands are
/// the original commands from the PTB file and not the commands that were parsed -- we will use
/// the string representation of the original commands to render errors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileIndexedErrors(pub BTreeMap<(String, usize), Vec<PTBCommand>>);

impl FileIndexedErrors {
    /// Take a set of commands and index them by file scope.
    pub fn new(commands: &BTreeMap<usize, PTBCommand>) -> Self {
        let mut file_indexed_commands = BTreeMap::new();
        let mut name_collision_count: BTreeMap<_, _> =
            [("console".to_owned(), 0)].into_iter().collect();
        let mut scope_stack = vec![];
        let mut current_scope = ("console".to_string(), 0);
        for (_, command) in commands {
            if command.name == FILE_START {
                // Push a dummy command to keep indices in line with the usage of the `--file`
                // command.
                file_indexed_commands
                    .entry(current_scope.clone())
                    .or_insert_with(Vec::new)
                    .push(command.clone());
                scope_stack.push(current_scope.clone());
                let name_index = name_collision_count
                    .entry(command.values[0].clone())
                    .and_modify(|i| *i += 1)
                    .or_insert(0);
                current_scope = (command.values[0].clone(), *name_index);
                continue;
            }

            if command.name == FILE_END {
                current_scope = scope_stack.pop().unwrap();
                continue;
            }

            file_indexed_commands
                .entry(current_scope.clone())
                .or_insert_with(Vec::new)
                .push(command.clone());
        }

        Self(file_indexed_commands)
    }

    pub fn get(&self, file_scope: &FileScope) -> Option<&PTBCommand> {
        self.0
            .get(&(file_scope.name.to_string(), file_scope.name_index))
            .and_then(|commands| commands.get(file_scope.file_command_index))
    }
}

impl Span {
    pub fn new(start: usize, end: usize, arg_idx: usize, file_scope: FileScope) -> Self {
        Self {
            start,
            end,
            arg_idx,
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
            arg_idx: self.arg_idx,
            file_scope: self.file_scope,
        }
    }

    /// Union a (possibly empty) set of spans into a single span. If the set of spans is empty,
    /// `None` is returned.
    pub fn union_spans(others: impl IntoIterator<Item = Span>) -> Option<Span> {
        let mut iter = others.into_iter();
        if let Some(first) = iter.next() {
            Some(first.union_with(iter))
        } else {
            None
        }
    }

    /// A span representing the command string of whichever file scope this span is in.
    pub fn cmd_span(cmd_len: usize, file_scope: FileScope) -> Span {
        Span {
            start: 0,
            end: cmd_len,
            // We add 1 to indices sometimes, so we need to subtract 1 here to make sure we don't
            // accidentally overflow.
            arg_idx: usize::MAX - 1,
            file_scope,
        }
    }

    pub fn is_cmd_span(&self) -> bool {
        self.arg_idx == usize::MAX - 1
    }

    /// Create a special span to represent an out-of-band error. These are errors that arise that
    /// cannot be directly attributed to a specific command in the PTB file. E.g., failing to set a
    /// gas budget.
    pub fn out_of_band_span() -> Span {
        Span {
            start: usize::MAX,
            end: usize::MAX,
            arg_idx: usize::MAX,
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

pub struct DisplayableError {
    pub command_string: String,
    pub label: LabeledSpan,
    pub error_string: String,
    pub help: Option<String>,
}

impl DisplayableError {
    // If no span we point to the command name
    // If there is a span, we convert the span range to the appropriate offset in the whole string for
    // the command
    pub fn new(original_command: PTBCommand, error: PTBError) -> Self {
        let PTBError::WithSource {
            span,
            message,
            help,
        } = error;
        let (range, command_string) = if span.is_out_of_band_span() {
            // Point to the command name
            (
                0..original_command.name.len(),
                original_command.name + " " + &original_command.values.join(" "),
            )
        } else {
            // Convert the character offset within the given argument index to the offset in the
            // whole string.
            let mut offset = original_command.name.len();
            let mut final_string = original_command.name.clone();
            let mut range = (span.start, span.end);
            for (i, arg) in original_command.values.iter().enumerate() {
                offset += 1;
                final_string.push_str(" ");
                if i != span.arg_idx {
                    offset += arg.len();
                    final_string.push_str(arg);
                } else {
                    range.0 += offset;
                    range.1 += offset;
                    offset += arg.len();
                    final_string.push_str(arg);
                }
            }

            // Handle range boundaries for e.g., unexpected tokens at the end of the token stream
            // by pushing on a space at the end of the string. This will capture any unexpected
            // token errors and allow us to point "to the end" of the argument.
            final_string.push_str(" ");
            (range.0..range.1, final_string)
        };
        let label = LabeledSpan::at(range, message.clone());
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
            command_string,
            label,
            error_string,
            help,
        }
    }

    pub fn create_report(self) -> miette::Report {
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
        .with_source_code(self.command_string)
    }
}

pub fn render_errors(
    commands: BTreeMap<usize, PTBCommand>,
    errors: Vec<PTBError>,
) -> Vec<miette::Report> {
    let file_indexed_commands = FileIndexedErrors::new(&commands);
    let mut rendered = vec![];
    for error in errors {
        let PTBError::WithSource { span, .. } = &error;
        let command_opt = file_indexed_commands.get(&span.file_scope);
        if span.is_out_of_band_span() || command_opt.is_none() {
            rendered.push(miette!(
                labels = vec![],
                "Error at command {} in PTB file '{}': {}",
                span.file_scope.file_command_index,
                span.file_scope.name,
                error
            ))
        } else {
            rendered
                .push(DisplayableError::new(command_opt.unwrap().clone(), error).create_report())
        }
    }
    rendered
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fmt::Debug;

use miette::{miette, LabeledSpan};
use thiserror::Error;

use crate::ptb::ptb::PTBCommand;

use super::{
    command_token::{FILE_END, FILE_START},
    context::FileScope,
};

#[macro_export]
macro_rules! error {
    ($x:expr, $($arg:tt)*) => {
        return Err($crate::err!($x, $($arg)*))
    };
    (sp: $l:expr, $x:expr, $($arg:tt)*) => {
        return Err($crate::err!(sp: $l, $x, $($arg)*))
    };
    (sp: $l:expr, help: { $($h:expr),* }, $x:expr, $($arg:tt)*) => {
        return Err($crate::err!(sp: $l, help: { $($h),* }, $x, $($arg)*))
    };
}

#[macro_export]
macro_rules! err {
    ($x:expr, $($arg:tt)*) => {
        $crate::ptb::ptb_parser::errors::PTBError::WithSource {
            file_scope: $x.context.current_file_scope().clone(),
            message: format!($($arg)*),
            span: None,
            help: None,
        }
    };
    (sp: $l:expr, $x:expr, $($arg:tt)*) => {
        $crate::ptb::ptb_parser::errors::PTBError::WithSource {
            file_scope: $x.context.current_file_scope().clone(),
            message: format!($($arg)*),
            span: Some($l),
            help: None,
        }
    };
    (sp: $l:expr, help: { $($h:expr),* }, $x:expr, $($arg:tt)*) => {
        $crate::ptb::ptb_parser::errors::PTBError::WithSource {
            file_scope: $x.context.current_file_scope().clone(),
            message: format!($($arg)*),
            span: Some($l),
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
    #[error("{message} at command {} in file '{}' {:?}", file_scope.file_command_index, file_scope.name, span)]
    WithSource {
        file_scope: FileScope,
        message: String,
        span: Option<Span>,
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
            PTBError::WithSource {
                file_scope,
                message,
                span,
                ..
            } => PTBError::WithSource {
                file_scope,
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
            .get(&(file_scope.name.clone(), file_scope.name_index))
            .and_then(|commands| commands.get(file_scope.file_command_index))
    }
}

impl Span {
    pub fn new(start: usize, end: usize, arg_idx: usize) -> Self {
        Self {
            start,
            end,
            arg_idx,
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
        }
    }
}

pub fn span<T: Debug + Clone + PartialEq + Eq>(loc: Span, value: T) -> Spanned<T> {
    Spanned { span: loc, value }
}

// If no span we point to the command name
// If there is a span, we convert the span range to the appropriate offset in the whole string for
// the command
fn convert_span(
    original_command: PTBCommand,
    error: PTBError,
) -> (String, LabeledSpan, String, Option<String>) {
    let PTBError::WithSource {
        span,
        file_scope,
        message,
        help,
    } = error;
    let (range, command_string) = match span {
        // No span -- point to the command name
        None => (
            0..original_command.name.len(),
            original_command.name + " " + &original_command.values.join(" "),
        ),
        Some(span) => {
            // Conver the character offset within the given argument index to the offset in the
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
        }
    };
    let label = LabeledSpan::at(range, message.clone());
    let error_string = format!(
        "{} {}",
        file_scope.file_command_index,
        if file_scope.name == "console" {
            "from console input".to_string()
        } else {
            let usage_string = if file_scope.name_index == 0 {
                "".to_string()
            } else {
                format!("(usage {} of file)", file_scope.name_index + 1)
            };
            format!("in PTB file '{}' {usage_string}", file_scope.name)
        }
    );

    (command_string, label, error_string, help)
}

pub fn convert_to_displayable_error(
    original_command: PTBCommand,
    error: PTBError,
) -> miette::Report {
    let (command_string, label, formatted_error, help_msg) = convert_span(original_command, error);
    match help_msg {
        Some(help_msg) => miette!(
            labels = vec![label],
            help = help_msg,
            "Error at command {}",
            formatted_error
        ),
        None => miette!(labels = vec![label], "Error at command {}", formatted_error),
    }
    .with_source_code(command_string)
}

pub fn render_errors(
    commands: BTreeMap<usize, PTBCommand>,
    errors: Vec<PTBError>,
) -> Vec<miette::Report> {
    let file_indexed_commands = FileIndexedErrors::new(&commands);
    let mut rendered = vec![];
    for error in errors {
        let file_scope = match &error {
            PTBError::WithSource { file_scope, .. } => file_scope,
        };
        match file_indexed_commands.get(&file_scope) {
            Some(command) => rendered.push(convert_to_displayable_error(command.clone(), error)),
            None => rendered.push(miette!(
                labels = vec![],
                "Error at command {} in PTB file '{}': {}",
                file_scope.file_command_index,
                file_scope.name,
                error
            )),
        }
    }
    rendered
}

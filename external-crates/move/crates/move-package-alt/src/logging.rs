// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! User-facing output for the package system.
//!
//! These are distinct from the `tracing` macros: `tracing` output is diagnostic and is not shown
//! to CLI users by default, so anything the user needs to read must go through these.

/// Print a user-facing warning: something is wrong but the operation continues.
pub fn print_warning(message: &str) {
    use ::colored::Colorize;
    eprintln!("[{}] {}", "WARNING".bold().yellow(), message);
}

/// Print a user-facing note: the operation is proceeding normally but made a choice worth knowing
/// about.
pub fn print_note(message: &str) {
    use ::colored::Colorize;
    eprintln!("[{}] {}", "NOTE".bold().yellow(), message);
}

/// Print user-facing information with no severity decoration.
pub fn print_info(message: &str) {
    eprintln!("{message}");
}

/// Print a user-facing error.
pub fn print_error(message: &str) {
    use ::colored::Colorize;
    eprintln!("[{}] {}", "ERROR".bold().red(), message);
}

#[macro_export]
macro_rules! user_warning {
    ($($arg:tt)*) => {{ $crate::logging::print_warning(&format!($($arg)*)); }};
}

#[macro_export]
macro_rules! user_note {
    ($($arg:tt)*) => {{ $crate::logging::print_note(&format!($($arg)*)); }};
}

#[macro_export]
macro_rules! user_info {
    ($($arg:tt)*) => {{ $crate::logging::print_info(&format!($($arg)*)); }};
}

#[macro_export]
macro_rules! user_error {
    ($($arg:tt)*) => {{ $crate::logging::print_error(&format!($($arg)*)); }};
}

pub(crate) use user_error;
pub(crate) use user_info;
pub(crate) use user_note;
pub(crate) use user_warning;

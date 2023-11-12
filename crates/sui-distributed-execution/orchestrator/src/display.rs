// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{fmt::Display, io::stdout};

use crossterm::{
    cursor::{RestorePosition, SavePosition},
    style::{Print, PrintStyledContent, Stylize},
    terminal::{Clear, ClearType},
};
use prettytable::format::{self};

pub fn header<S: Display>(message: S) {
    if cfg!(not(test)) {
        crossterm::execute!(
            stdout(),
            PrintStyledContent(format!("\n{message}\n").green().bold()),
        )
        .unwrap();
    }
}

pub fn error<S: Display>(message: S) {
    if cfg!(not(test)) {
        crossterm::execute!(
            stdout(),
            PrintStyledContent(format!("\n{message}\n").red().bold()),
        )
        .unwrap();
    }
}

pub fn warn<S: Display>(message: S) {
    if cfg!(not(test)) {
        crossterm::execute!(
            stdout(),
            PrintStyledContent(format!("\n{message}\n").bold()),
        )
        .unwrap();
    }
}

pub fn config<N: Display, V: Display>(name: N, value: V) {
    if cfg!(not(test)) {
        crossterm::execute!(
            stdout(),
            PrintStyledContent(format!("{name}: ").bold()),
            Print(format!("{value}\n"))
        )
        .unwrap();
    }
}

pub fn action<S: Display>(message: S) {
    if cfg!(not(test)) {
        crossterm::execute!(stdout(), Print(format!("{message} ... ")), SavePosition).unwrap();
    }
}

pub fn status<S: Display>(status: S) {
    if cfg!(not(test)) {
        crossterm::execute!(
            stdout(),
            RestorePosition,
            SavePosition,
            Clear(ClearType::UntilNewLine),
            Print(format!("[{status}]"))
        )
        .unwrap();
    }
}

pub fn done() {
    if cfg!(not(test)) {
        crossterm::execute!(
            stdout(),
            RestorePosition,
            Clear(ClearType::UntilNewLine),
            Print(format!("[{}]\n", "Ok".green()))
        )
        .unwrap();
    }
}

pub fn newline() {
    if cfg!(not(test)) {
        crossterm::execute!(stdout(), Print("\n")).unwrap();
    }
}

/// Default style for tables printed to stdout.
pub fn default_table_format() -> format::TableFormat {
    format::FormatBuilder::new()
        .separators(
            &[
                format::LinePosition::Top,
                format::LinePosition::Bottom,
                format::LinePosition::Title,
            ],
            format::LineSeparator::new('-', '-', '-', '-'),
        )
        .padding(1, 1)
        .build()
}

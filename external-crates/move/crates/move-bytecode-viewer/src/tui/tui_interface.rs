// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]
use std::{error::Error, io};

use crossterm::{
    event::EnableMouseCapture,
    execute,
    terminal::{enable_raw_mode, EnterAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, text::Line, Terminal};

use crate::tui::TUI;

/// The output that will be display in the TUI. The text in the `left_screen`
/// and `right_screen` fields will be displayed on the left screen and right
/// screen respectively.
#[derive(Debug, Clone)]
pub struct TUIOutput<'a> {
    /// The text to be displayed on the left screen.
    pub left_screen: Vec<Line<'a>>,
    /// The text to be displayed on the right screen.
    pub right_screen: Vec<Line<'a>>,
}

pub trait TUIInterface {
    /// The title to be used for the left screen
    const LEFT_TITLE: &'static str;

    /// The title to be used for the right screen
    const RIGHT_TITLE: &'static str;

    /// Function called on each redraw. The `TUIOutput` contains that updated
    /// data to display on each pane.
    fn on_redraw(&mut self, line_number: u16, column_number: u16) -> TUIOutput;

    /// Bounds the line number so that it does not run past the text.
    fn bound_line(&self, line_number: u16) -> u16;

    /// Bounds the column number (w.r.t. the current `line_number`) so that the
    /// cursor does not overrun the line.
    fn bound_column(&self, line_number: u16, column_number: u16) -> u16;
}

/// A Debugging interface for the TUI. Useful for debugging things.
#[derive(Debug, Clone)]
pub struct DebugInterface {
    text: Vec<String>,
}

impl DebugInterface {
    pub fn new(text: String) -> Self {
        let text: Vec<_> = text.split('\n').map(|x| x.to_string()).collect();
        Self { text }
    }
}

impl TUIInterface for DebugInterface {
    const LEFT_TITLE: &'static str = "Left pane";
    const RIGHT_TITLE: &'static str = "Right pane";
    fn on_redraw(&mut self, line_number: u16, column_number: u16) -> TUIOutput {
        TUIOutput {
            left_screen: self
                .text
                .iter()
                .map(AsRef::as_ref)
                .map(Line::from)
                .collect(),
            right_screen: vec![Line::from(format!(
                "line number: {}   column number: {}",
                line_number, column_number
            ))],
        }
    }

    fn bound_line(&self, line_number: u16) -> u16 {
        std::cmp::min(
            line_number,
            (self.text.len().checked_sub(1).unwrap()) as u16,
        )
    }

    fn bound_column(&self, line_number: u16, column_number: u16) -> u16 {
        std::cmp::min(column_number, self.text[line_number as usize].len() as u16)
    }
}

/// Starts a two-pane TUI using the provided `Interface` to update the screen
/// according to cursor movements.
pub fn start_tui_with_interface<Interface: TUIInterface>(
    interface: Interface,
) -> Result<(), Box<dyn Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let mut tui = TUI::new(interface);
    loop {
        terminal.draw(|frame| tui.redraw(frame))?;
        if tui.handle_input()? {
            break;
        }
    }
    Ok(())
}

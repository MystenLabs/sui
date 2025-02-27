// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]
use ratatui::{
    style::Style,
    text::{Line, Span},
};

/// A `TextBuilder` is used to build up a paragraph, where some parts of it may
/// need to have different styling, and where this styling may not conform to
/// line boundaries.
#[derive(Debug, Clone, Default)]
pub struct TextBuilder<'a> {
    lines: Vec<Line<'a>>,
}

impl<'a> TextBuilder<'a> {
    /// Create a new text builder
    pub fn new() -> Self {
        Self { lines: Vec::new() }
    }

    /// Add `text` with the given style to the text builder. This functions
    /// tracks newlines in the text already recorded, and will splice lines
    /// between the previous text and the new `text` being added. It
    /// respects the `style` of both the old text and the newly added text.
    pub fn add(&mut self, text: String, style: Style) {
        let into_lines = |string: String| {
            string
                .split('\n')
                .map(|x| x.to_string())
                .map(|x| Line::styled(x, style))
                .collect::<Vec<_>>()
        };
        let last_line_ends_with_newline = self
            .lines
            .last()
            .map(|last_line| {
                last_line
                    .spans
                    .last()
                    .map(|last_span| last_span.content.ends_with('\n'))
                    .unwrap_or(false)
            })
            .unwrap_or(true);

        if !last_line_ends_with_newline {
            let mut iter = text.splitn(2, '\n');
            iter.next().into_iter().for_each(|line_continuation| {
                self.lines
                    .last_mut()
                    .unwrap()
                    .push_span(Span::styled(line_continuation.to_string(), style));
            });
            iter.next().into_iter().for_each(|remainder| {
                self.lines.extend(into_lines(remainder.to_string()));
            });
        } else {
            self.lines.extend(into_lines(text))
        }
    }

    /// Return the final lines.
    pub fn finish(self) -> Vec<Line<'a>> {
        self.lines
    }
}

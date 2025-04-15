// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// TODO move this to docgen once this stabilizes

use codespan::{ByteIndex, ByteOffset, RawIndex, RawOffset};
use std::{cell::RefCell, collections::BTreeMap, ops::Deref};

/// A label which can be created at the code writers current output position to later insert
/// code at this position.
#[derive(Debug, Clone, Copy)]
pub struct CodeWriterLabel(ByteIndex);

pub struct CodeWriter {
    /// The generated output string.
    output: String,

    /// Current active indentation.
    indent: usize,

    /// A map from label indices to the current position in output they are pointing to.
    label_map: BTreeMap<ByteIndex, ByteIndex>,
}

pub struct CodeWriterRefCell(pub RefCell<CodeWriter>);

impl CodeWriter {
    /// Creates new code writer, with the given default location.
    pub fn new() -> CodeWriter {
        Self {
            output: String::new(),
            indent: 0,
            label_map: BTreeMap::new(),
        }
    }

    /// Returns a label for the end of the current output.
    pub fn create_label(&mut self) -> CodeWriterLabel {
        let index = ByteIndex(self.output.len() as RawIndex);
        self.label_map.insert(index, index);
        CodeWriterLabel(index)
    }

    /// O(n) operation to insert a string at a label.
    pub fn insert_at_label(&mut self, label: CodeWriterLabel, s: impl AsRef<str>) {
        let s: &str = s.as_ref();
        let index = *self.label_map.get(&label.0).expect("label undefined");
        let shift = ByteOffset(s.len() as RawOffset);
        // Shift indices after index.
        for idx in self.label_map.values_mut() {
            if *idx > index {
                *idx += shift;
            }
        }
        self.output.insert_str(index.0 as usize, s);
    }

    /// Calls a function to process the code written so far.
    pub fn process_result<T, F: FnMut(&str) -> T>(&self, mut f: F) -> T {
        // Ensure that result is terminated by newline without spaces.
        // This assumes that we already trimmed all individual lines.
        let output = self.output.as_str();
        let mut end = output.trim_end().len();
        if end < output.len() && output[end..].starts_with('\n') {
            end += 1;
        }
        f(&output[0..end])
    }

    /// Extracts the output as a string. Leaves the writers data empty.
    pub fn extract_result(&mut self) -> String {
        let mut output = std::mem::take(&mut self.output);
        // Eliminate any empty lines at end, but keep the lest EOL
        output.truncate(output.trim_end().len());
        if !output.ends_with('\n') {
            output.push('\n');
        }
        output
    }

    /// Indents any subsequently written output. The current line of output and any subsequent ones
    /// will be indented. Note this works after the last output was `\n` but the line is still
    /// empty.
    pub fn indent(&mut self) {
        self.indent += 4;
    }

    /// Undo previously done indentation.
    pub fn unindent(&mut self) {
        self.indent -= 4;
    }

    /// Emit a string, broken down into lines to apply current indentation.
    pub fn emit(&mut self, s: impl AsRef<str>) {
        let s = s.as_ref();
        // str::lines ignores trailing newline, so deal with this ad-hoc
        let ends_in_newline = s.ends_with('\n');
        let mut lines = s.lines();
        if let Some(first) = lines.next() {
            self.emit_str(first);
        }
        for l in lines {
            Self::trim_trailing_whitespace(&mut self.output);
            self.output.push('\n');
            self.emit_str(l);
        }

        if ends_in_newline {
            self.output.push('\n');
        }
    }

    fn trim_trailing_whitespace(s: &mut String) {
        s.truncate(s.trim_end().len());
    }

    /// Helper for emitting a string for a single line.
    fn emit_str(&mut self, s: &str) {
        // If we are looking at the beginning of a new line, emit indent now.
        if self.indent > 0 && (self.output.is_empty() || self.output.ends_with('\n')) {
            self.output.push_str(&" ".repeat(self.indent));
        }
        self.output.push_str(s);
    }
}

impl std::fmt::Write for CodeWriter {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.emit(s);
        Ok(())
    }
}

impl std::fmt::Write for CodeWriterRefCell {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.0.borrow_mut().emit(s);
        Ok(())
    }
}

impl Deref for CodeWriterRefCell {
    type Target = RefCell<CodeWriter>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

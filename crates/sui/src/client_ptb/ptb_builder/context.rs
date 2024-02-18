// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_symbol_pool::Symbol;
use std::path::PathBuf;

/// A `FileScope` represents a command in a given file, along with dismbiguating if there are
/// multiple occurences of the same file in the PTB.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileScope {
    pub file_command_index: usize,
    /// Intern names
    pub name: Symbol,
    /// Since the same filename may appear multiple times this disambiguates between which usage of
    /// the file we are in.
    pub name_index: usize,
}

impl FileScope {
    pub fn increment_file_command_index(&mut self) {
        self.file_command_index += 1;
    }

    /// Qualify a path with the current file scope. This means that relative file paths inside of
    /// PTBs will be respected and resolved correctly.
    pub fn qualify_path(&self, path: &str) -> PathBuf {
        let command_ptb_path = PathBuf::from(self.name.as_str());
        let mut qual_package_path = match command_ptb_path.parent() {
            None => PathBuf::new(),
            Some(x) if x.to_string_lossy().is_empty() => PathBuf::new(),
            Some(parent) => parent.to_path_buf(),
        };
        qual_package_path.push(path);
        qual_package_path
    }
}

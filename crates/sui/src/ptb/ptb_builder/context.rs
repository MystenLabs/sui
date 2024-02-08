// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::ptb::ptb_builder::errors::{PTBError, PTBResult, Span};
use move_symbol_pool::Symbol;
use std::{collections::BTreeMap, path::PathBuf};

/// A `FileScope` represents a command in a given file, along with dismbiguating if there are
/// multiple occurences of the same file in the PTB.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileScope {
    pub file_command_index: usize,
    /// Intern names
    pub name: Symbol,
    /// Since the same filename may appear twice this disambiguates between first, second, etc.
    pub name_index: usize,
}

/// A `PTBContext` is a context holds the current file scope and a stack of file scopes that we are
/// under currently. It also holds a map of seen scopes to disambiguate between different usages of
/// the same PTB file possibly.
#[derive(Debug, Clone)]
pub struct PTBContext {
    /// The current file scope that we are in right now.
    current_file_scope: FileScope,
    /// The stack of file scopes that we are under currently.
    file_scopes: Vec<FileScope>,
    /// The map of seen scopes to disambiguate between different usages of the same PTB file possibly.
    seen_scopes: BTreeMap<Symbol, usize>,
}

impl PTBContext {
    pub fn new() -> Self {
        Self {
            current_file_scope: FileScope {
                file_command_index: 0,
                name: Symbol::from("console"),
                name_index: 0,
            },
            file_scopes: vec![],
            seen_scopes: [(Symbol::from("console"), 0)].into_iter().collect(),
        }
    }

    /// Push the current scope onto the stack of scopes, and put us into a new scope `name` at
    /// command 0.
    pub fn push_file_scope(&mut self, name: String) {
        // Account for the `--file` command itself in the index
        self.increment_file_command_index();
        let name_symbol = Symbol::from(name);
        let name_index = self
            .seen_scopes
            .entry(name_symbol)
            .and_modify(|i| *i += 1)
            .or_insert(0);
        let scope = std::mem::replace(
            &mut self.current_file_scope,
            FileScope {
                file_command_index: 0,
                name: name_symbol,
                name_index: *name_index,
            },
        );
        self.file_scopes.push(scope);
    }

    /// Pop the current scope off the stack of scopes, and put us into the previous scope. Errors
    /// if we somehow try to pop off more scopes than we have (which means internal logic is wrong
    /// somewhere).
    pub fn pop_file_scope(&mut self, name: &str) -> PTBResult<()> {
        let name_symbol = Symbol::from(name);
        if self.current_file_scope.name != name_symbol {
            return Err(PTBError::WithSource {
                message: format!(
                    "Internal Error: Expected file scope '{}' but got '{}'",
                    name_symbol, self.current_file_scope.name
                ),
                span: Span::cmd_span(0, self.current_file_scope()),
                help: None,
            });
        }
        let scope = self.file_scopes.pop().ok_or_else(|| PTBError::WithSource {
            message: "Internal Error: No file scopes to pop".to_owned(),
            span: Span::cmd_span(0, self.current_file_scope()),
            help: None,
        })?;
        self.current_file_scope = scope;
        Ok(())
    }

    pub fn current_file_scope(&self) -> FileScope {
        self.current_file_scope
    }

    pub fn increment_file_command_index(&mut self) {
        self.current_file_scope.file_command_index += 1;
    }
}

impl FileScope {
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

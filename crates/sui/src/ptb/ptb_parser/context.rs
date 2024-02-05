// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use move_symbol_pool::Symbol;

use crate::ptb::ptb_parser::errors::PTBError;

use super::errors::{PTBResult, Span};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileScope {
    pub file_command_index: usize,
    /// Intern names
    pub name: Symbol,
    /// Since the same filename may appear twice this disambiguates between first, second, etc.
    pub name_index: usize,
}

#[derive(Debug, Clone)]
pub struct PTBContext {
    current_file_scope: FileScope,
    file_scopes: Vec<FileScope>,
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

    pub fn pop_file_scope(&mut self, name: &str) -> PTBResult<()> {
        let name_symbol = Symbol::from(name);
        if self.current_file_scope.name != name_symbol {
            return Err(PTBError::WithSource {
                message: format!(
                    "ICE: Expected file scope '{}' but got '{}'",
                    name_symbol, self.current_file_scope.name
                ),
                span: Span::cmd_span(0, self.current_file_scope()),
                help: None,
            });
        }
        let scope = self.file_scopes.pop().ok_or_else(|| PTBError::WithSource {
            message: "ICE: No file scopes to pop".to_owned(),
            span: Span::cmd_span(0, self.current_file_scope()),
            help: None,
        })?;
        self.current_file_scope = scope;
        Ok(())
    }

    pub fn current_file_scope(&self) -> FileScope {
        self.current_file_scope
    }

    pub fn current_command_index(&self) -> usize {
        self.current_file_scope.file_command_index
    }

    pub fn increment_file_command_index(&mut self) {
        self.current_file_scope.file_command_index += 1;
    }

    pub fn current_location(&self) -> String {
        format!(
            "command {} in file {}",
            self.current_file_scope.file_command_index, self.current_file_scope.name
        )
    }
}

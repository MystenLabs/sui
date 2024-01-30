// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use crate::ptb::ptb_parser::errors::PTBError;

use super::errors::PTBResult;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileScope {
    pub file_command_index: usize,
    pub name: String,
    // Since the same filename may appear twice this disambiguates between first, second, etc.
    pub name_index: usize,
}

#[derive(Debug, Clone)]
pub struct PTBContext {
    current_file_scope: FileScope,
    file_scopes: Vec<FileScope>,
    seen_scopes: BTreeMap<String, usize>,
}

impl PTBContext {
    pub fn new() -> Self {
        Self {
            current_file_scope: FileScope {
                file_command_index: 0,
                name: "console".to_owned(),
                name_index: 0,
            },
            file_scopes: vec![],
            seen_scopes: [("console".to_owned(), 0)].into_iter().collect(),
        }
    }

    pub fn push_file_scope(&mut self, name: String) {
        // Account for the `--file` command itself in the index
        self.increment_file_command_index();
        let name_index = self
            .seen_scopes
            .entry(name.clone())
            .and_modify(|i| *i += 1)
            .or_insert(0);
        let scope = std::mem::replace(
            &mut self.current_file_scope,
            FileScope {
                file_command_index: 0,
                name,
                name_index: *name_index,
            },
        );
        self.file_scopes.push(scope);
    }

    pub fn pop_file_scope(&mut self, name: &str) -> PTBResult<()> {
        if self.current_file_scope.name != name {
            return Err(PTBError::WithSource {
                file_scope: self.current_file_scope.clone(),
                message: format!(
                    "ICE: Expected file scope '{}' but got '{}'",
                    name, self.current_file_scope.name
                ),
                span: None,
                help: None,
            });
        }
        let scope = self.file_scopes.pop().ok_or_else(|| PTBError::WithSource {
            file_scope: self.current_file_scope.clone(),
            message: "ICE: No file scopes to pop".to_owned(),
            span: None,
            help: None,
        })?;
        self.current_file_scope = scope;
        Ok(())
    }

    pub fn current_file_scope(&self) -> &FileScope {
        &self.current_file_scope
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

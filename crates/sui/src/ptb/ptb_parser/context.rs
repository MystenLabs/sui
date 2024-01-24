// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::ptb::ptb_parser::errors::PTBError;

use super::errors::PTBResult;

#[derive(Debug, Clone)]
pub struct FileScope {
    pub file_command_index: usize,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct PTBContext {
    current_file_scope: FileScope,
    file_scopes: Vec<FileScope>,
}

impl PTBContext {
    pub fn new() -> Self {
        Self {
            current_file_scope: FileScope {
                file_command_index: 0,
                name: "console".to_owned(),
            },
            file_scopes: vec![],
        }
    }

    pub fn push_file_scope(&mut self, name: String) {
        let scope = std::mem::replace(
            &mut self.current_file_scope,
            FileScope {
                file_command_index: 0,
                name,
            },
        );
        self.file_scopes.push(scope);
    }

    pub fn pop_file_scope(&mut self, name: String) -> PTBResult<()> {
        if self.current_file_scope.name != name {
            return Err(PTBError::WithSource {
                file_scope: self.current_file_scope.clone(),
                message: format!(
                    "ICE: Expected file scope '{}' but got '{}'",
                    name, self.current_file_scope.name
                ),
            });
        }
        let scope = self.file_scopes.pop().ok_or_else(|| PTBError::WithSource {
            file_scope: self.current_file_scope.clone(),
            message: "ICE: No file scopes to pop".to_owned(),
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

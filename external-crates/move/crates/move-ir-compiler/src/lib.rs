// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

pub mod util;

#[cfg(test)]
mod unit_tests;

use std::collections::BTreeMap;

use anyhow::Result;
use move_binary_format::file_format::CompiledModule;
use move_bytecode_source_map::source_map::SourceMap;
use move_core_types::account_address::AccountAddress;
use move_ir_to_bytecode::{compiler::compile_module, parser::parse_module_with_named_addresses};

/// An API for the compiler. Supports setting custom options.
#[derive(Clone, Debug)]
pub struct Compiler<'a> {
    /// Extra dependencies to compile with.
    pub deps: Vec<&'a CompiledModule>,
    pub named_addresses: BTreeMap<String, AccountAddress>,
}

impl<'a> Compiler<'a> {
    pub fn new(deps: Vec<&'a CompiledModule>) -> Self {
        Self {
            deps,
            named_addresses: BTreeMap::new(),
        }
    }

    pub fn with_named_addresses(
        mut self,
        named_addresses: BTreeMap<String, AccountAddress>,
    ) -> Self {
        self.named_addresses.extend(named_addresses);
        self
    }

    /// Compiles the module.
    pub fn into_compiled_module(self, code: &str) -> Result<CompiledModule> {
        Ok(self.compile_mod(code)?.0)
    }

    /// Compiles the module into a serialized form.
    pub fn into_module_blob(self, code: &str) -> Result<Vec<u8>> {
        let compiled_module = self.compile_mod(code)?.0;

        let mut serialized_module = Vec::<u8>::new();
        compiled_module.serialize_with_version(compiled_module.version, &mut serialized_module)?;
        Ok(serialized_module)
    }

    fn compile_mod(self, code: &str) -> Result<(CompiledModule, SourceMap)> {
        let parsed_module = parse_module_with_named_addresses(code, &self.named_addresses)?;
        let (compiled_module, source_map) =
            compile_module(parsed_module, self.deps.iter().copied())?;
        Ok((compiled_module, source_map))
    }
}

// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

pub mod util;

#[cfg(test)]
mod unit_tests;

use anyhow::Result;
use move_binary_format::file_format::CompiledModule;
use move_bytecode_source_map::source_map::SourceMap;
use move_ir_to_bytecode::{compiler::compile_module, parser::parse_module};

/// An API for the compiler. Supports setting custom options.
#[derive(Clone, Debug)]
pub struct Compiler<'a> {
    /// Extra dependencies to compile with.
    pub deps: Vec<&'a CompiledModule>,
}

impl<'a> Compiler<'a> {
    pub fn new(deps: Vec<&'a CompiledModule>) -> Self {
        Self { deps }
    }

    /// Compiles the module.
    pub fn into_compiled_module(self, code: &str) -> Result<CompiledModule> {
        Ok(self.compile_mod(code)?.0)
    }

    /// Compiles the module into a serialized form.
    pub fn into_module_blob(self, code: &str) -> Result<Vec<u8>> {
        let compiled_module = self.compile_mod(code)?.0;

        let mut serialized_module = Vec::<u8>::new();
        compiled_module.serialize(&mut serialized_module)?;
        Ok(serialized_module)
    }

    fn compile_mod(self, code: &str) -> Result<(CompiledModule, SourceMap)> {
        let parsed_module = parse_module(code)?;
        let (compiled_module, source_map) =
            compile_module(parsed_module, self.deps.iter().copied())?;
        Ok((compiled_module, source_map))
    }
}

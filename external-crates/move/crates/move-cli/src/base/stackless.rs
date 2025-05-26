// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use clap::*;
use move_binary_format::CompiledModule;
use move_command_line_common::files::{extension_equals, find_filenames, MOVE_COMPILED_EXTENSION};
use move_model_2::{self as M2};
use move_package::BuildConfig;
use move_stackless_bytecode_2::{
    stackless_bytecode::StacklessBytecode,
    disassembler::Disassembler
};

use std::{
    collections::BTreeMap,
    path::{Path}
};

const DEFAULT_OUTPUT_DIRECTORY: &str = "stackless_bytecode";

/// Generate a serialized summary of a Move package (e.g., functions, structs, annotations, etc.)
#[derive(Parser)]
#[clap(name = "stackless")]
pub struct Stackless {    
    /// Directory that all generated summaries should be nested under.
    #[clap(long = "output-directory", value_name = "PATH", default_value = DEFAULT_OUTPUT_DIRECTORY)]
    output_directory: String,
}

impl Stackless {
    pub fn execute(self, path: Option<&Path>, _build_config: BuildConfig) -> anyhow::Result<()> {
        let input_path = path.unwrap_or_else(|| Path::new("."));
        let bytecode_files = find_filenames(&[input_path], |path| {
            extension_equals(path, MOVE_COMPILED_EXTENSION)
        })?;

        let mut modules = Vec::new();

        for bytecode_file in &bytecode_files {
            let bytes = std::fs::read(bytecode_file)?;
            println!("Deserializing bytecode file: {}", bytecode_file);
            let module = CompiledModule::deserialize_with_defaults(&bytes)?;
            modules.push(module);
        }

        let model_compiled = M2::compiled_model::Model::from_compiled(&BTreeMap::new(), modules);

        // TODO wip, dissambler to be replaced with stackless bytecode
        let disassembler = Disassembler::new(model_compiled);

        disassembler.disassemble()?;

        Ok(())
    }
}
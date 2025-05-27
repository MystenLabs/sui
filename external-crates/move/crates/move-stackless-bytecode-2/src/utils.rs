// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::file_format::CompiledModule;
use move_disassembler::disassembler::Disassembler;
use move_ir_types::location::Spanned;

use std::{
    fs::File,
    io::{BufReader, Read},
    path::{Path, PathBuf},
};

#[derive(Clone, Debug)]
pub struct Utils {
    pub(crate) module_path: PathBuf,
    pub(crate) deserialized_module: CompiledModule,
    pub(crate) disassembled: String,
}

impl Utils {
    pub fn new(module_path: PathBuf) -> Self {
        Self {
            module_path,
            deserialized_module: CompiledModule::default(),
            disassembled: String::new(),
        }
    }

    pub fn get_module_path(&self) -> &PathBuf {
        &self.module_path
    }

    pub fn get_deserialized_module(&self) -> &CompiledModule {
        &self.deserialized_module
    }

    pub fn get_disassembled(&self) -> &String {
        &self.disassembled
    }

    pub fn deserialize(&self) -> anyhow::Result<CompiledModule> {
        deserialize(&self.module_path)
    }

    pub fn disassemble(&self) -> anyhow::Result<String> {
        disassemble(&self.deserialized_module)
    }
}

impl Default for Utils {
    fn default() -> Self {
        Self {
            module_path: PathBuf::new(),
            deserialized_module: CompiledModule::default(),
            disassembled: String::new(),
        }
    }
}

pub(crate) fn deserialize(module_path: &PathBuf) -> anyhow::Result<CompiledModule> {
    assert!(Path::new(&module_path).exists(), "Bad path to .mv file");
    let mut bytes = Vec::new();
    let mut file = BufReader::new(File::open(module_path)?);
    file.read_to_end(&mut bytes)?;
    // this deserialized a module to the max version of the bytecode but it's OK here because
    // it's not run as part of the deterministic replicated state machine.
    Ok(CompiledModule::deserialize_with_defaults(&bytes)?)
}

pub(crate) fn disassemble(module: &CompiledModule) -> anyhow::Result<String> {
    let d = Disassembler::from_module(module, Spanned::unsafe_no_loc(()).loc)?;
    let disassemble_string = d.disassemble()?;
    // let (disassemble_string, _) = d.disassemble_with_source_map()?;

    // println!("{}", disassemble_string);
    Ok(disassemble_string)
}

pub(crate) fn comma_separated<T: std::fmt::Display>(items: &[T]) -> String {
    items
        .iter()
        .map(|item| format!("{}", item))
        .collect::<Vec<_>>()
        .join(", ")
}

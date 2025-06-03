// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use clap::*;
use move_binary_format::CompiledModule;
use move_command_line_common::files::{MOVE_COMPILED_EXTENSION, extension_equals, find_filenames};
use move_package::BuildConfig;
use move_stackless_bytecode_2::generator::StacklessBytecodeGenerator;

use std::path::{Path, PathBuf};

/// Generate a serialized summary of a Move package (e.g., functions, structs, annotations, etc.)
#[derive(Parser)]
#[clap(name = "stackless")]
pub struct Stackless {
    #[arg(name = "legacy-stackless", long = "legacy-stackless")]
    legacy_stackless: bool,

    #[arg(name = "legacy-disassemble", long = "legacy-disassemble")]
    legacy_disassemble: bool,

    #[arg(name = "disassemble", long = "disassemble")]
    disassemble: bool,

    #[arg(name = "module_path", long = "module")]
    module_path: Option<PathBuf>,
}

impl Stackless {
    pub fn execute(self, path: Option<&Path>, _build_config: BuildConfig) -> anyhow::Result<()> {
        let bytecode_files = if self
            .module_path
            .as_deref()
            .is_some_and(|path| path.exists())
        {
            let input_path = self.module_path.as_deref().unwrap();
            vec![input_path.to_str().unwrap().to_string()]
        } else {
            let input_path = path.unwrap_or_else(|| Path::new("."));
            find_filenames(&[input_path], |path| {
                extension_equals(path, MOVE_COMPILED_EXTENSION)
            })?
        };

        let mut modules = Vec::new();

        for bytecode_file in &bytecode_files {
            let bytes = std::fs::read(bytecode_file)?;
            let module = CompiledModule::deserialize_with_defaults(&bytes)?;
            modules.push(module);
        }

        let stackless = StacklessBytecodeGenerator::new(modules);

        if self.legacy_stackless {
            return stackless.legacy_stackless();
        }

        if self.legacy_disassemble {
            let disassembled_modules = stackless.legacy_disassemble()?;
            for disassembled_mod in disassembled_modules {
                println!("{}", disassembled_mod);
            }
            return Ok(());
        }

        if self.disassemble {
            return stackless.disassemble_source();
        }

        stackless.execute()
    }
}

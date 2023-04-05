// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use super::reroot_path;
use clap::*;
use move_compiler::compiled_unit::{CompiledUnit, CompiledUnitEnum, NamedCompiledModule};
use move_disassembler::disassembler::Disassembler;
use move_package::{compilation::compiled_package::CompiledUnitWithSource, BuildConfig};
use std::path::PathBuf;

/// Disassemble the Move bytecode pointed to
#[derive(Parser)]
#[clap(name = "disassemble")]
pub struct Disassemble {
    /// Start a disassembled bytecode-to-source explorer
    #[clap(long = "interactive")]
    pub interactive: bool,
    /// The package name. If not provided defaults to current package modules only
    #[clap(long = "package")]
    pub package_name: Option<String>,
    /// The name of the module or script in the package to disassemble
    #[clap(long = "name")]
    pub module_or_script_name: String,
    /// Also print the raw disassembly using Rust's Debug output, at the end.
    #[clap(long = "Xdebug")]
    pub debug: bool,
}

impl Disassemble {
    pub fn execute(self, path: Option<PathBuf>, config: BuildConfig) -> anyhow::Result<()> {
        let rerooted_path = reroot_path(path)?;
        let Self {
            interactive,
            package_name,
            module_or_script_name,
            debug,
        } = self;
        // Make sure the package is built
        let package = config.compile_package(&rerooted_path, &mut Vec::new())?;
        let needle_package = package_name
            .as_deref()
            .unwrap_or(package.compiled_package_info.package_name.as_str());
        match package
            .get_module_by_name(needle_package, &module_or_script_name)
            .or_else(|_| package.get_script_by_name(needle_package, &module_or_script_name))
            .ok()
        {
            None => anyhow::bail!(
                "Unable to find module or script with name '{}' in package '{}'",
                module_or_script_name,
                needle_package,
            ),
            Some(unit) => {
                // Once we find the compiled bytecode we're interested in, startup the bytecode
                // viewer, run the disassembler, or display the debug output, depending on args.
                if interactive {
                    match unit {
                        CompiledUnitWithSource {
                            unit:
                                CompiledUnit::Module(NamedCompiledModule {
                                    module, source_map, ..
                                }),
                            source_path,
                        } => move_bytecode_viewer::start_viewer_in_memory(
                            module.clone(),
                            source_map.clone(),
                            source_path,
                        ),
                        _ => anyhow::bail!("Interactive disassembler not supported for scripts"),
                    }
                } else {
                    println!("{}", Disassembler::from_unit(&unit.unit).disassemble()?);
                    if debug {
                        match &unit.unit {
                            CompiledUnitEnum::Module(module) => println!("\n{:#?}", module.module),
                            CompiledUnitEnum::Script(script) => println!("\n{:#?}", script.script),
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

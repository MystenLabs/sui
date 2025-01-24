// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use super::reroot_path;
use clap::*;
use move_bytecode_source_map::utils::serialize_to_json_string;
use move_compiler::compiled_unit::NamedCompiledModule;
use move_disassembler::disassembler::Disassembler;
use move_package::{compilation::compiled_package::CompiledUnitWithSource, BuildConfig};
use std::path::Path;

/// Disassemble the Move bytecode pointed to
#[derive(Parser)]
#[clap(name = "disassemble")]
pub struct Disassemble {
    #[clap(long = "interactive")]
    /// Start a disassembled bytecode-to-source explorer
    pub interactive: bool,
    #[clap(long = "package")]
    /// The package name. If not provided defaults to current package modules only
    pub package_name: Option<String>,
    #[clap(long = "name")]
    /// The name of the module or script in the package to disassemble
    pub module_or_script_name: String,
    #[clap(long = "Xdebug")]
    /// Also print the raw disassembly using Rust's Debug output, at the end.
    pub debug: bool,
    #[clap(long = "bytecode-map")]
    /// Print the "bytecode map" (source map for disassembled bytecode)
    pub bytecode_map: bool,
}

impl Disassemble {
    pub fn execute(self, path: Option<&Path>, config: BuildConfig) -> anyhow::Result<()> {
        let rerooted_path = reroot_path(path)?;
        let Self {
            interactive,
            package_name,
            module_or_script_name,
            debug,
            bytecode_map,
        } = self;
        // Make sure the package is built
        let package = config.compile_package(&rerooted_path, &mut Vec::new())?;
        let needle_package = package_name
            .as_deref()
            .unwrap_or(package.compiled_package_info.package_name.as_str());
        match package
            .get_module_by_name(needle_package, &module_or_script_name)
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
                    let CompiledUnitWithSource {
                        unit:
                            NamedCompiledModule {
                                module, source_map, ..
                            },
                        source_path,
                    } = unit;
                    move_bytecode_viewer::start_viewer_in_memory(
                        module.clone(),
                        source_map.clone(),
                        source_path,
                    )
                } else {
                    let d = Disassembler::from_unit(&unit.unit);
                    let (disassemble_string, bcode_map) = d.disassemble_with_source_map()?;
                    if bytecode_map {
                        println!("{}", serialize_to_json_string(&bcode_map)?);
                    }
                    println!("{}", disassemble_string);
                    if debug {
                        println!("\n{:#?}", &unit.unit.module)
                    }
                }
            }
        }
        Ok(())
    }
}

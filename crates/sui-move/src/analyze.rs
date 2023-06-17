// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use move_binary_format::{normalized, CompiledModule};
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use sui_json_rpc_types::SuiRawMovePackage;

#[derive(Parser)]
pub struct Analyze {
    #[clap(long)]
    json_path: PathBuf,
}

// TODO:
// total Display?
// total OTW?
// total coins?
#[derive(Debug)]
pub struct Stats {
    /// Total number of packages, including multiple versions of the same package
    pub total_packages: usize,
    /// Same as above, but counting multiple versions of the same package as a single package
    pub total_distinct_packages: usize,
    /// Total bytes of all bytecode modules (does not inlcude package metadata)
    pub total_module_bytes: usize,
    /// Total number of modules
    pub total_modules: usize,
    /// Total number of structs
    pub total_structs: usize,
    /// Total number of struct declarations that are Sui objects (i.e., have `key`)
    pub total_objects: usize,
    // TODO: this is also counting OTW's
    /// Total number of struct declarations that are hot potatoes (i.e., have no abilities)
    pub total_hot_potatoes: usize,
    pub total_functions: usize,
    /// Maximum number of modules in a single package
    pub max_modules: usize,
    /// Maximum number of structs in a single package
    pub max_structs: usize,
    /// Maximum number of functions in a single package
    pub max_functions: usize,
    /// Maximum number of fields in a single struct declaration
    pub max_struct_fields: usize,
    /// Maximum number of function parameters in a single function declaration
    pub max_function_parameters: usize,
    /// Maximum number of dependencies for a single package
    pub max_dependencies: usize,
    /// Highest package version (i.e., max times a single package has been upgraded)
    pub max_version: usize,
}

impl Analyze {
    pub fn execute(self) -> anyhow::Result<()> {
        let file = File::open(self.json_path)?;
        let reader = BufReader::new(file);
        let packages: Vec<SuiRawMovePackage> = serde_json::from_reader(reader)?;
        let total_packages = packages.len();
        let mut total_distinct_packages = 0;
        let mut total_module_bytes = 0;
        let mut total_modules = 0;
        let mut total_structs = 0;
        let mut total_objects = 0;
        let mut total_hot_potatoes = 0;
        let mut total_functions = 0;
        let mut max_modules = 0;
        let mut max_structs = 0;
        let mut max_functions = 0;
        let mut max_function_parameters = 0;
        let mut max_struct_fields = 0;
        let mut max_dependencies = 0;
        let mut max_version = 0;
        for pkg in packages {
            if pkg.version == 1.into() {
                total_distinct_packages += 1;
            }
            max_version = std::cmp::max(max_version, pkg.version.into());
            total_modules += pkg.module_map.len();
            max_modules = std::cmp::max(max_modules, pkg.module_map.len());
            max_dependencies = std::cmp::max(max_dependencies, pkg.linkage_table.len());
            println!("=========");
            println!("ID {}", pkg.id);
            for (name, bytes) in pkg.module_map {
                println!("Module {}", name);
                let module = CompiledModule::deserialize_with_defaults(&bytes).unwrap();
                let normalized_module = normalized::Module::new(&module);
                if !normalized_module.structs.is_empty() {
                    println!("  Structs:")
                }
                for (name, _struct_def) in &normalized_module.structs {
                    println!("    {}", name)
                }
                let string_constants: Vec<Vec<u8>> = normalized_module
                    .constants
                    .iter()
                    .filter_map(|c| {
                        if c.type_.is_vec_u8() {
                            Some(
                                c.data
                                    .clone()
                                    .into_iter()
                                    .filter(|b| b.is_ascii_graphic())
                                    .collect::<Vec<u8>>(),
                            )
                        } else {
                            None
                        }
                    })
                    .collect();
                if !string_constants.is_empty() {
                    println!("  String constants:");
                }
                for constant in string_constants {
                    match std::str::from_utf8(constant.as_slice()) {
                        Ok(v) => println!("    {}", v),
                        Err(_) => (), // not a string, we don't care
                    }
                }
                total_module_bytes += bytes.len();
                total_structs += normalized_module.functions.len();
                total_objects += normalized_module
                    .structs
                    .iter()
                    .filter(|(_, s)| s.abilities.has_key())
                    .count();
                total_hot_potatoes += normalized_module
                    .structs
                    .iter()
                    .filter(|(_, s)| s.abilities.into_u8() == 0)
                    .count();
                total_functions += normalized_module.structs.len();
                max_structs = std::cmp::max(max_structs, normalized_module.structs.len());
                max_functions = std::cmp::max(max_functions, normalized_module.functions.len());
                max_function_parameters = std::cmp::max(
                    max_function_parameters,
                    normalized_module
                        .functions
                        .iter()
                        .map(|(_, f)| f.parameters.len())
                        .max()
                        .unwrap_or(0),
                );
                max_struct_fields = std::cmp::max(
                    max_struct_fields,
                    normalized_module
                        .structs
                        .iter()
                        .map(|(_, s)| s.fields.len())
                        .max()
                        .unwrap_or(0),
                );
            }
        }
        let stats = Stats {
            total_packages,
            total_distinct_packages,
            total_module_bytes,
            total_modules,
            total_structs,
            total_objects,
            total_hot_potatoes,
            total_functions,
            max_modules,
            max_structs,
            max_functions,
            max_struct_fields,
            max_function_parameters,
            max_dependencies,
            max_version,
        };
        println!("Stats: {:?}", stats);
        Ok(())
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use move_binary_format::CompiledModule;
use move_bytecode_source_map::utils::serialize_to_json_string;
use move_cli::base;
use move_disassembler::disassembler::Disassembler;
use move_ir_types::location::Spanned;
use move_package::BuildConfig;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use std::path::PathBuf;

#[derive(Parser)]
#[group(id = "sui-move-disassemmble")]
pub struct Disassemble {
    /// Path to a .mv file to disassemble
    #[clap(name = "module_path")]
    module_path: PathBuf,

    /// Whether to display the disassembly in raw Debug format
    #[clap(long = "Xdebug")]
    debug: bool,

    #[clap(short = 'i', long = "interactive")]
    interactive: bool,

    /// Print the "bytecode map" (source map for disassembled bytecode)
    #[clap(long = "bytecode-map")]
    pub bytecode_map: bool,
}

impl Disassemble {
    pub fn execute(
        self,
        package_path: Option<&Path>,
        build_config: BuildConfig,
    ) -> anyhow::Result<()> {
        if base::reroot_path(Some(&self.module_path)).is_ok() {
            // disassembling bytecode inside the source package that produced it--use the source info
            let module_name = self
                .module_path
                .file_stem()
                .expect("Bad module path")
                .to_str()
                .expect("Cannot convert module name to string")
                .to_owned();
            move_cli::base::disassemble::Disassemble {
                interactive: self.interactive,
                package_name: None,
                module_or_script_name: module_name,
                debug: self.debug,
                bytecode_map: self.bytecode_map,
            }
            .execute(package_path, build_config)?;
            return Ok(());
        }

        // disassembling a bytecode file with no source info
        assert!(
            Path::new(&self.module_path).exists(),
            "Bad path to .mv file"
        );

        let mut bytes = Vec::new();
        let mut file = BufReader::new(File::open(self.module_path)?);
        file.read_to_end(&mut bytes)?;
        // this deserialized a module to the max version of the bytecode but it's OK here because
        // it's not run as part of the deterministic replicated state machine.
        let module = CompiledModule::deserialize_with_defaults(&bytes)?;

        if self.debug {
            println!("{module:#?}");
        } else {
            let d = Disassembler::from_module(&module, Spanned::unsafe_no_loc(()).loc)?;
            let (disassemble_string, bcode_map) = d.disassemble_with_source_map()?;
            if self.bytecode_map {
                println!("{}", serialize_to_json_string(&bcode_map)?);
            }
            println!("{}", disassemble_string);
        }

        Ok(())
    }
}

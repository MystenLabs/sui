// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use clap::*;
use move_command_line_common::files::verify_and_create_named_address_mapping;
use move_compiler::{
    command_line::{self as cli},
    shared::{self, Flags, NumericalAddress},
};

#[derive(Debug, Parser)]
#[clap(
    name = "move-build",
    about = "Compile Move source to Move bytecode",
    author,
    version
)]
pub struct Options {
    /// The source files to check and compile
    #[clap(
        name = "PATH_TO_SOURCE_FILE",
        takes_value(true),
        multiple_values(true),
        multiple_occurrences(true)
    )]
    pub source_files: Vec<String>,

    /// The library files needed as dependencies
    #[clap(
        name = "PATH_TO_DEPENDENCY_FILE",
        short = cli::DEPENDENCY_SHORT,
        long = cli::DEPENDENCY,
    )]
    pub dependencies: Vec<String>,

    /// The Move bytecode output directory
    #[clap(
        name = "PATH_TO_OUTPUT_DIRECTORY",
        short = cli::OUT_DIR_SHORT,
        long = cli::OUT_DIR,
        default_value = cli::DEFAULT_OUTPUT_DIR,
    )]
    pub out_dir: String,

    /// Save bytecode source map to disk
    #[clap(
        name = "",
        short = cli::SOURCE_MAP_SHORT,
        long = cli::SOURCE_MAP,
    )]
    pub emit_source_map: bool,

    /// Named address mapping
    #[clap(
        name = "NAMED_ADDRESSES",
        short = 'a',
        long = "addresses",
        parse(try_from_str = shared::parse_named_address)
    )]
    pub named_addresses: Vec<(String, NumericalAddress)>,

    #[clap(flatten)]
    pub flags: Flags,
}

pub fn main() -> anyhow::Result<()> {
    let Options {
        source_files,
        dependencies,
        out_dir,
        emit_source_map,
        flags,
        named_addresses,
    } = Options::parse();

    let interface_files_dir = format!("{}/generated_interface_files", out_dir);
    let named_addr_map = verify_and_create_named_address_mapping(named_addresses)?;
    let bytecode_version = flags.bytecode_version();
    let (files, compiled_units) =
        move_compiler::Compiler::from_files(source_files, dependencies, named_addr_map)
            .set_interface_files_dir(interface_files_dir)
            .set_flags(flags)
            .build_and_report()?;
    move_compiler::output_compiled_units(
        bytecode_version,
        emit_source_map,
        files,
        compiled_units,
        &out_dir,
    )
}

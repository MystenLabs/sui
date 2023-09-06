// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{path::PathBuf, str::FromStr};

use crate::{experimental, sandbox::utils::PackageContext, Move};
use anyhow::Result;
use move_core_types::{
    language_storage::TypeTag, parser, transaction_argument::TransactionArgument,
};
use std::path::Path;

use clap::{Parser, ValueEnum};

#[derive(Parser)]
pub enum ExperimentalCommand {
    /// Perform a read/write set analysis and print the results for
    /// `module_file`::`script_name`.
    #[clap(name = "read-write-set")]
    ReadWriteSet {
        /// Path to .mv file containing module bytecode.
        #[clap(name = "module")]
        module_file: PathBuf,
        /// A function inside `module_file`.
        #[clap(name = "function")]
        fun_name: String,
        #[clap(
            long = "signers",
            num_args(1..),
            action = clap::ArgAction::Append,
        )]
        signers: Vec<String>,
        #[clap(
            long = "args",
            value_parser = parser::parse_transaction_argument,
            num_args(1..),
            action = clap::ArgAction::Append,
        )]
        args: Vec<TransactionArgument>,
        #[clap(
            long = "type-args",
            value_parser = parser::parse_type_tag,
            num_args(1..),
            action = clap::ArgAction::Append,
        )]
        type_args: Vec<TypeTag>,
        #[clap(
            long = "concretize",
            value_enum,
            ignore_case = true,
            default_value = "dont"
        )]
        concretize: ConcretizeMode,
    },
}

// Specify if/how the analysis should concretize and filter the static analysis summary

// Specify if/how the analysis should concretize and filter the static analysis summary
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ConcretizeMode {
    // Show the full concretized access paths read or written (e.g. 0xA/0x1::M::S/f/g)
    Paths,
    // Show only the concrete resource keys that are read (e.g. 0xA/0x1::M::S)
    Reads,
    // Show only the concrete resource keys that are written (e.g. 0xA/0x1::M::S)
    Writes,
    // Do not concretize; show the results from the static analysis
    Dont,
}

impl FromStr for ConcretizeMode {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "paths" => Ok(ConcretizeMode::Paths),
            "reads" => Ok(ConcretizeMode::Reads),
            "writes" => Ok(ConcretizeMode::Writes),
            "dont" => Ok(ConcretizeMode::Dont),
            _ => Err(anyhow::anyhow!("Invalid concretize mode: {}", s)),
        }
    }
}

impl ExperimentalCommand {
    pub fn handle_command(&self, move_args: &Move, storage_dir: &Path) -> Result<()> {
        match self {
            ExperimentalCommand::ReadWriteSet {
                module_file,
                fun_name,
                signers,
                args,
                type_args,
                concretize,
            } => {
                let state = PackageContext::new(&move_args.package_path, &move_args.build_config)?
                    .prepare_state(storage_dir)?;
                experimental::commands::analyze_read_write_set(
                    &state,
                    module_file,
                    fun_name,
                    signers,
                    args,
                    type_args,
                    *concretize,
                    move_args.verbose,
                )
            }
        }
    }
}

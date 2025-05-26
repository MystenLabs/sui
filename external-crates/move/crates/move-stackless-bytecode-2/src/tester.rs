// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;

use crate::utils::{deserialize, disassemble};

use std::path::Path;
use std::path::PathBuf;

#[derive(Parser)]
struct Args {
    /// Path to a .mv file to disassemble
    #[arg(name = "module_path")]
    module_path: PathBuf,
    /// Whether to display the disassembly in raw Debug format
    #[arg(long = "debug")]
    debug: bool,
}

pub fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    assert!(
        Path::new(&args.module_path).exists(),
        "Bad path to .mv file"
    );

    let deserialized_module = deserialize(&args.module_path)?;
    // println!("Deserialized module: {:?}", deserialized_module);

    let mut modules = Vec::new();
    modules.push(deserialized_module.clone());

    let disassembled = disassemble(&deserialized_module)?;
    if args.debug {
        println!("{}", disassembled);
    }

    Ok(())
}

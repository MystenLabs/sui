// Copyright (c) Verichains, 2023

use std::fs;

use clap::Parser;

use move_binary_format::file_format::CompiledModule;

use move_decompiler::decompiler::{aptos_compat::BinaryIndexedView, Decompiler, OptimizerSettings};

#[derive(Debug, Parser)]
#[clap(author, version, about)]
struct Args {
    // Input files
    #[clap(short = 'b', long = "bytecode")]
    pub files: Vec<String>,

    #[clap(
        long = "disable-variable-declaration-optimization",
        default_value = "false"
    )]
    pub disable_variable_declaration_optimization: bool,
}

enum CompiledBinary {
    Module(CompiledModule),
}

fn main() {
    let args = Args::parse();

    let binaries_store: Vec<_> = args
        .files
        .iter()
        .map(|file| {
            let bytecode_bytes = fs::read(file).unwrap_or_else(|err| {
                panic!("Error: failed to read file {}: {}", file.to_string(), err);
            });

            CompiledBinary::Module(CompiledModule::deserialize_with_defaults(&bytecode_bytes).unwrap_or_else(
                |err| {
                    panic!("Error: failed to deserialize module blob: {}", err);
                },
            ))
        })
        .collect();

    let binaries: Vec<_> = binaries_store
        .iter()
        .map(|binary| match binary {
            CompiledBinary::Module(module) => BinaryIndexedView::Module(module),
        })
        .collect();

    let mut decompiler = Decompiler::new(
        binaries,
        OptimizerSettings {
            disable_optimize_variables_declaration: args.disable_variable_declaration_optimization,
        },
    );
    let output = decompiler.decompile().expect("Error: unable to decompile");
    println!("{}", output);
}

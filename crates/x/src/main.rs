// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use clap::Parser;

mod lint;

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(subcommand)]
    cmd: Command,
}

#[derive(Debug, Parser)]
enum Command {
    #[clap(name = "lint")]
    /// Run lints
    Lint(lint::Args),
}

fn main() -> Result<()> {
    let args = Args::parse();

    match args.cmd {
        Command::Lint(args) => lint::run(args),
    }
}

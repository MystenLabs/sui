// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use clap::Parser;

mod external_crates_tests;
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
    #[clap(name = "external-crates-tests")]
    /// Run external crate tests
    ExternalCratesTests,
}

fn main() -> Result<()> {
    let args = Args::parse();

    match args.cmd {
        Command::Lint(args) => lint::run(args),
        Command::ExternalCratesTests => external_crates_tests::run(),
    }
}

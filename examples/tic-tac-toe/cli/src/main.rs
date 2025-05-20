// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use clap::Parser;
use tic_tac_toe::command::Command;

#[derive(Parser, Debug)]
#[arg(
    name = env!("CARGO_BIN_NAME"),
    about = "A CLI for playing tic-tac-toe on-chain.",
)]
struct Args {
    #[command(subcommand)]]
    command: Command,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    args.command.execute().await?;
    Ok(())
}

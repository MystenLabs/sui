// // Copyright (c) Mysten Labs, Inc.
// // SPDX-License-Identifier: Apache-2.0

use clap::*;
use colored::Colorize;
use node_stream::commands::NodeStreamCommand;
use sui_types::exit_main;

#[tokio::main]
async fn main() {
    #[cfg(windows)]
    colored::control::set_virtual_terminal(true).unwrap();

    let cmd: NodeStreamCommand = NodeStreamCommand::parse();


    exit_main!(cmd.execute().await);
}
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
extern crate core;

use std::path::PathBuf;

use clap::*;

#[cfg(test)]
#[path = "../unit_tests/cli_tests.rs"]
mod cli_tests;

#[derive(Parser)]
#[clap(
    name = "Sui Store Dump",
    about = "Dumps store tables",
    rename_all = "kebab-case"
)]
struct StoreOpt {
    /// Path of the DB to dump
    #[clap(name = "db_path")]
    db_path: String,
    /// If this is a gateway DB or authority DB
    #[clap(name = "gateway", long)]
    gateway: bool,
    /// The name of the table to dump
    #[clap(name = "table_name")]
    table_name: String,
}

#[tokio::main]
async fn main() {
    let options: StoreOpt = StoreOpt::parse();
    let mp = dump_table(
        options.gateway,
        PathBuf::from(options.db_path),
        &options.table_name,
    );
    for (k, v) in mp {
        println!("{:?}: {:?}", k, v);
    }
}
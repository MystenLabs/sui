// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use self::db_dump::{dump_table, list_tables};
use clap::Parser;
use std::path::PathBuf;

pub mod db_dump;

#[derive(Parser)]
#[clap(rename_all = "kebab-case")]
pub enum DbToolCommand {
    ListTables,
    Dump(Dump),
}

#[derive(Parser)]
pub struct Dump {
    /// If this is a gateway DB or authority DB
    #[clap(long = "gateway")]
    gateway: bool,
    /// The name of the table to dump
    #[clap(long = "table_name")]
    table_name: String,
}

pub fn execute_db_tool_command(db_path: PathBuf, cmd: DbToolCommand) -> anyhow::Result<()> {
    match cmd {
        DbToolCommand::ListTables => print_db_all_tables(db_path),
        DbToolCommand::Dump(d) => print_all_entries(d.gateway, db_path, &d.table_name),
    }
}

pub fn print_db_all_tables(db_path: PathBuf) -> anyhow::Result<()> {
    list_tables(db_path)?.iter().for_each(|t| println!("{}", t));
    Ok(())
}

pub fn print_all_entries(gateway: bool, path: PathBuf, table_name: &str) -> anyhow::Result<()> {
    for (k, v) in dump_table(gateway, path, table_name)? {
        println!("{:>100?}: {:?}", k, v);
    }
    Ok(())
}

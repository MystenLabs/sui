// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use self::db_dump::{dump_table, list_tables, StoreName};
use clap::Parser;
use std::path::PathBuf;
use sui_types::base_types::EpochId;

pub mod db_dump;

#[derive(Parser)]
#[clap(rename_all = "kebab-case")]
pub enum DbToolCommand {
    ListTables,
    Dump(Dump),
}

#[derive(Parser)]
#[clap(rename_all = "kebab-case")]
pub struct Dump {
    /// The type of store to dump
    #[clap(long = "store")]
    store_name: StoreName,
    /// The name of the table to dump
    #[clap(long = "table-name")]
    table_name: String,
    /// The size of page to dump. This is a u16
    #[clap(long = "page-size")]
    page_size: u16,
    /// The page number to dump
    #[clap(long = "page-num")]
    page_number: usize,

    // TODO: We should load this automatically from the system object in AuthorityPerpetualTables.
    // This is very difficult to do right now because you can't share code between
    // AuthorityPerpetualTables and AuthorityEpochTablesReadonly.
    /// The epoch to use when loading AuthorityEpochTables.
    #[clap(long = "epoch")]
    epoch: Option<EpochId>,
}

pub fn execute_db_tool_command(db_path: PathBuf, cmd: DbToolCommand) -> anyhow::Result<()> {
    match cmd {
        DbToolCommand::ListTables => print_db_all_tables(db_path),
        DbToolCommand::Dump(d) => print_all_entries(
            d.store_name,
            d.epoch,
            db_path,
            &d.table_name,
            d.page_size,
            d.page_number,
        ),
    }
}

pub fn print_db_all_tables(db_path: PathBuf) -> anyhow::Result<()> {
    list_tables(db_path)?.iter().for_each(|t| println!("{}", t));
    Ok(())
}

pub fn print_all_entries(
    store: StoreName,
    epoch: Option<EpochId>,
    path: PathBuf,
    table_name: &str,
    page_size: u16,
    page_number: usize,
) -> anyhow::Result<()> {
    for (k, v) in dump_table(store, epoch, path, table_name, page_size, page_number)? {
        println!("{:>100?}: {:?}", k, v);
    }
    Ok(())
}

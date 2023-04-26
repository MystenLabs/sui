// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use self::db_dump::{dump_table, duplicate_objects_summary, list_tables, table_summary, StoreName};
use crate::db_tool::db_dump::{compact, print_table_metadata};
use clap::Parser;
use std::path::{Path, PathBuf};
use sui_core::authority::authority_store_tables::AuthorityPerpetualTables;
use sui_core::checkpoints::CheckpointStore;
use sui_types::base_types::EpochId;
use typed_store::rocks::MetricConf;

pub mod db_dump;

#[derive(Parser)]
#[clap(rename_all = "kebab-case")]
pub enum DbToolCommand {
    ListTables,
    Dump(Options),
    TableSummary(Options),
    DuplicatesSummary,
    ResetDB,
    ListDBMetadata(Options),
    Compact,
}

#[derive(Parser)]
#[clap(rename_all = "kebab-case")]
pub struct Options {
    /// The type of store to dump
    #[clap(long = "store", value_enum)]
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
        DbToolCommand::TableSummary(d) => {
            print_db_table_summary(d.store_name, d.epoch, db_path, &d.table_name)
        }
        DbToolCommand::DuplicatesSummary => print_db_duplicates_summary(db_path),
        DbToolCommand::ResetDB => reset_db_to_genesis(&db_path),
        DbToolCommand::ListDBMetadata(d) => {
            print_table_metadata(d.store_name, d.epoch, db_path, &d.table_name)
        }
        DbToolCommand::Compact => compact(db_path),
    }
}

pub fn print_db_all_tables(db_path: PathBuf) -> anyhow::Result<()> {
    list_tables(db_path)?.iter().for_each(|t| println!("{}", t));
    Ok(())
}

pub fn print_db_duplicates_summary(db_path: PathBuf) -> anyhow::Result<()> {
    let (total_count, duplicate_count, total_bytes, duplicated_bytes) =
        duplicate_objects_summary(db_path);
    println!(
        "Total objects = {}, duplicated objects = {}, total bytes = {}, duplicated bytes = {}",
        total_count, duplicate_count, total_bytes, duplicated_bytes
    );
    Ok(())
}

pub fn reset_db_to_genesis(path: &Path) -> anyhow::Result<()> {
    // Follow the below steps to test:
    //
    // Get a db snapshot. Either generate one by running stress locally and enabling db checkpoints or download one from S3 bucket (pretty big in size though).
    // Download the snapshot for the epoch you want to restore to the local disk. You will find one snapshot per epoch in the S3 bucket. We need to place the snapshot in the dir where config is pointing to. If db-config in fullnode.yaml is /opt/sui/db/authorities_db and we want to restore from epoch 10, we want to copy the snapshot to /opt/sui/db/authorities_dblike this:
    // aws s3 cp s3://myBucket/dir /opt/sui/db/authorities_db/ --recursive —exclude “*” —include “epoch_10*”
    // Mark downloaded snapshot as live: mv  /opt/sui/db/authorities_db/epoch_10  /opt/sui/db/authorities_db/live
    // Reset the downloaded db to execute from genesis with: cargo run --package sui-tool -- db-tool --db-path /opt/sui/db/authorities_db/live reset-db
    // Start the sui full node: cargo run --release --bin sui-node -- --config-path ~/db_checkpoints/fullnode.yaml
    // A sample fullnode.yaml config would be:
    // ---
    // db-path:  /opt/sui/db/authorities_db
    // network-address: /ip4/0.0.0.0/tcp/8080/http
    // json-rpc-address: "0.0.0.0:9000"
    // websocket-address: "0.0.0.0:9001"
    // metrics-address: "0.0.0.0:9184"
    // admin-interface-port: 1337
    // enable-event-processing: true
    // grpc-load-shed: ~
    // grpc-concurrency-limit: ~
    // p2p-config:
    //   listen-address: "0.0.0.0:8084"
    // genesis:
    //   genesis-file-location:  <path to genesis blob for the network>
    // authority-store-pruning-config:
    //   num-latest-epoch-dbs-to-retain: 3
    //   epoch-db-pruning-period-secs: 3600
    //   num-epochs-to-retain: 18446744073709551615
    //   max-checkpoints-in-batch: 10
    //   max-transactions-in-batch: 1000
    //   use-range-deletion: true
    let path = path.join("store").join("perpetual");

    let perpetual_db = AuthorityPerpetualTables::open_tables_read_write(
        path.join("store").join("perpetual"),
        MetricConf::default(),
        None,
        None,
    );
    perpetual_db.reset_db_for_execution_since_genesis()?;

    let checkpoint_db = CheckpointStore::open_tables_read_write(
        path.join("checkpoints"),
        MetricConf::default(),
        None,
        None,
    );
    checkpoint_db.reset_db_for_execution_since_genesis()?;
    Ok(())
}

pub fn print_db_table_summary(
    store: StoreName,
    epoch: Option<EpochId>,
    path: PathBuf,
    table_name: &str,
) -> anyhow::Result<()> {
    let summary = table_summary(store, epoch, path, table_name)?;
    let quantiles = vec![25, 50, 75, 90, 99];
    println!(
        "Total num keys = {}, total key bytes = {}, total value bytes = {}",
        summary.num_keys, summary.key_bytes_total, summary.value_bytes_total
    );
    println!("Key size distribution:\n");
    quantiles.iter().for_each(|q| {
        println!(
            "p{:?} -> {:?} bytes\n",
            q,
            summary.key_hist.value_at_quantile(*q as f64 / 100.0)
        );
    });
    println!("Value size distribution:\n");
    quantiles.iter().for_each(|q| {
        println!(
            "p{:?} -> {:?} bytes\n",
            q,
            summary.value_hist.value_at_quantile(*q as f64 / 100.0)
        );
    });
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

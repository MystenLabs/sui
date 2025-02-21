// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use self::db_dump::{dump_table, duplicate_objects_summary, list_tables, table_summary, StoreName};
use self::index_search::{search_index, SearchRange};
use crate::db_tool::db_dump::{compact, print_table_metadata, prune_checkpoints, prune_objects};
use anyhow::{anyhow, bail};
use clap::Parser;
use std::path::{Path, PathBuf};
use sui_core::authority::authority_per_epoch_store::AuthorityEpochTables;
use sui_core::authority::authority_store_tables::AuthorityPerpetualTables;
use sui_core::checkpoints::CheckpointStore;
use sui_types::base_types::{EpochId, ObjectID};
use sui_types::digests::{CheckpointContentsDigest, TransactionDigest};
use sui_types::effects::TransactionEffectsAPI;
use sui_types::messages_checkpoint::{CheckpointDigest, CheckpointSequenceNumber};
use typed_store::rocks::MetricConf;
pub mod db_dump;
mod index_search;

#[derive(Parser)]
#[command(rename_all = "kebab-case")]
pub enum DbToolCommand {
    ListTables,
    Dump(Options),
    IndexSearchKeyRange(IndexSearchKeyRangeOptions),
    IndexSearchCount(IndexSearchCountOptions),
    TableSummary(Options),
    DuplicatesSummary,
    ListDBMetadata(Options),
    PrintLastConsensusIndex,
    PrintConsensusCommit(PrintConsensusCommitOptions),
    PrintTransaction(PrintTransactionOptions),
    PrintObject(PrintObjectOptions),
    PrintCheckpoint(PrintCheckpointOptions),
    PrintCheckpointContent(PrintCheckpointContentOptions),
    ResetDB,
    RewindCheckpointExecution(RewindCheckpointExecutionOptions),
    Compact,
    PruneObjects,
    PruneCheckpoints,
    SetCheckpointWatermark(SetCheckpointWatermarkOptions),
}

#[derive(Parser)]
#[command(rename_all = "kebab-case")]
pub struct IndexSearchKeyRangeOptions {
    #[arg(long = "table-name", short = 't')]
    table_name: String,
    #[arg(long = "start", short = 's')]
    start: String,
    #[arg(long = "end", short = 'e')]
    end_key: String,
}

#[derive(Parser)]
#[command(rename_all = "kebab-case")]
pub struct IndexSearchCountOptions {
    #[arg(long = "table-name", short = 't')]
    table_name: String,
    #[arg(long = "start", short = 's')]
    start: String,
    #[arg(long = "count", short = 'c')]
    count: u64,
}

#[derive(Parser)]
#[command(rename_all = "kebab-case")]
pub struct Options {
    /// The type of store to dump
    #[arg(long = "store", short = 's', value_enum)]
    store_name: StoreName,
    /// The name of the table to dump
    #[arg(long = "table-name", short = 't')]
    table_name: String,
    /// The size of page to dump. This is a u16
    #[arg(long = "page-size", short = 'p')]
    page_size: u16,
    /// The page number to dump
    #[arg(long = "page-num", short = 'n')]
    page_number: usize,

    // TODO: We should load this automatically from the system object in AuthorityPerpetualTables.
    // This is very difficult to do right now because you can't share code between
    // AuthorityPerpetualTables and AuthorityEpochTablesReadonly.
    /// The epoch to use when loading AuthorityEpochTables.
    #[arg(long = "epoch", short = 'e')]
    epoch: Option<EpochId>,
}

#[derive(Parser)]
#[command(rename_all = "kebab-case")]
pub struct PrintConsensusCommitOptions {
    #[arg(long, help = "Sequence number of the consensus commit")]
    seqnum: u64,
}

#[derive(Parser)]
#[command(rename_all = "kebab-case")]
pub struct PrintTransactionOptions {
    #[arg(long, help = "The transaction digest to print")]
    digest: TransactionDigest,
}

#[derive(Parser)]
#[command(rename_all = "kebab-case")]
pub struct PrintObjectOptions {
    #[arg(long, help = "The object id to print")]
    id: ObjectID,
    #[arg(long, help = "The object version to print")]
    version: Option<u64>,
}

#[derive(Parser)]
#[command(rename_all = "kebab-case")]
pub struct PrintCheckpointOptions {
    #[arg(long, help = "The checkpoint digest to print")]
    digest: CheckpointDigest,
}

#[derive(Parser)]
#[command(rename_all = "kebab-case")]
pub struct PrintCheckpointContentOptions {
    #[arg(
        long,
        help = "The checkpoint content digest (NOT the checkpoint digest)"
    )]
    digest: CheckpointContentsDigest,
}

#[derive(Parser)]
#[command(rename_all = "kebab-case")]
pub struct RemoveTransactionOptions {
    #[arg(long, help = "The transaction digest to remove")]
    digest: TransactionDigest,

    #[arg(long)]
    confirm: bool,

    /// The epoch to use when loading AuthorityEpochTables.
    /// Defaults to the current epoch.
    #[arg(long = "epoch", short = 'e')]
    epoch: Option<EpochId>,
}

#[derive(Parser)]
#[command(rename_all = "kebab-case")]
pub struct RemoveObjectLockOptions {
    #[arg(long, help = "The object ID to remove")]
    id: ObjectID,

    #[arg(long, help = "The object version to remove")]
    version: u64,

    #[arg(long)]
    confirm: bool,
}

#[derive(Parser)]
#[command(rename_all = "kebab-case")]
pub struct RewindCheckpointExecutionOptions {
    #[arg(long = "epoch")]
    epoch: EpochId,

    #[arg(long = "checkpoint-sequence-number")]
    checkpoint_sequence_number: u64,
}

#[derive(Parser)]
#[command(rename_all = "kebab-case")]
pub struct SetCheckpointWatermarkOptions {
    #[arg(long)]
    highest_verified: Option<CheckpointSequenceNumber>,

    #[arg(long)]
    highest_synced: Option<CheckpointSequenceNumber>,
}

pub async fn execute_db_tool_command(db_path: PathBuf, cmd: DbToolCommand) -> anyhow::Result<()> {
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
        DbToolCommand::ListDBMetadata(d) => {
            print_table_metadata(d.store_name, d.epoch, db_path, &d.table_name)
        }
        DbToolCommand::PrintLastConsensusIndex => print_last_consensus_index(&db_path),
        DbToolCommand::PrintConsensusCommit(d) => print_consensus_commit(&db_path, d),
        DbToolCommand::PrintTransaction(d) => print_transaction(&db_path, d),
        DbToolCommand::PrintObject(o) => print_object(&db_path, o),
        DbToolCommand::PrintCheckpoint(d) => print_checkpoint(&db_path, d),
        DbToolCommand::PrintCheckpointContent(d) => print_checkpoint_content(&db_path, d),
        DbToolCommand::ResetDB => reset_db_to_genesis(&db_path),
        DbToolCommand::RewindCheckpointExecution(d) => {
            rewind_checkpoint_execution(&db_path, d.epoch, d.checkpoint_sequence_number)
        }
        DbToolCommand::Compact => compact(db_path),
        DbToolCommand::PruneObjects => prune_objects(db_path).await,
        DbToolCommand::PruneCheckpoints => prune_checkpoints(db_path).await,
        DbToolCommand::IndexSearchKeyRange(rg) => {
            let res = search_index(
                db_path,
                rg.table_name,
                rg.start,
                SearchRange::ExclusiveLastKey(rg.end_key),
            )?;
            for (k, v) in res {
                println!("{}: {}", k, v);
            }
            Ok(())
        }
        DbToolCommand::IndexSearchCount(sc) => {
            let res = search_index(
                db_path,
                sc.table_name,
                sc.start,
                SearchRange::Count(sc.count),
            )?;
            for (k, v) in res {
                println!("{}: {}", k, v);
            }
            Ok(())
        }
        DbToolCommand::SetCheckpointWatermark(d) => set_checkpoint_watermark(&db_path, d),
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

pub fn print_last_consensus_index(path: &Path) -> anyhow::Result<()> {
    let epoch_tables = AuthorityEpochTables::open_tables_read_write(
        path.to_path_buf(),
        MetricConf::default(),
        None,
        None,
    );
    let last_index = epoch_tables.get_last_consensus_index()?;
    println!("Last consensus index is {:?}", last_index);
    Ok(())
}

// TODO: implement for consensus.
pub fn print_consensus_commit(
    _path: &Path,
    _opt: PrintConsensusCommitOptions,
) -> anyhow::Result<()> {
    println!("Printing consensus commit is unimplemented");
    Ok(())
}

pub fn print_transaction(path: &Path, opt: PrintTransactionOptions) -> anyhow::Result<()> {
    let perpetual_db = AuthorityPerpetualTables::open(&path.join("store"), None);
    if let Some((epoch, checkpoint_seq_num)) =
        perpetual_db.get_checkpoint_sequence_number(&opt.digest)?
    {
        println!(
            "Transaction {:?} executed in epoch {} checkpoint {}",
            opt.digest, epoch, checkpoint_seq_num
        );
    };
    if let Some(effects) = perpetual_db.get_effects(&opt.digest)? {
        println!(
            "Transaction {:?} dependencies: {:#?}",
            opt.digest,
            effects.dependencies(),
        );
    };
    Ok(())
}

pub fn print_object(path: &Path, opt: PrintObjectOptions) -> anyhow::Result<()> {
    let perpetual_db = AuthorityPerpetualTables::open(&path.join("store"), None);

    let obj = if let Some(version) = opt.version {
        perpetual_db.get_object_by_key_fallible(&opt.id, version.into())?
    } else {
        perpetual_db.get_object_fallible(&opt.id)?
    };

    if let Some(obj) = obj {
        println!("Object {:?}:\n{:#?}", opt.id, obj);
    } else {
        println!("Object {:?} not found", opt.id);
    }

    Ok(())
}

pub fn print_checkpoint(path: &Path, opt: PrintCheckpointOptions) -> anyhow::Result<()> {
    let checkpoint_store = CheckpointStore::new(&path.join("checkpoints"));
    let checkpoint = checkpoint_store
        .get_checkpoint_by_digest(&opt.digest)?
        .ok_or(anyhow!(
            "Checkpoint digest {:?} not found in checkpoint store",
            opt.digest
        ))?;
    println!("Checkpoint: {:?}", checkpoint);
    drop(checkpoint_store);
    print_checkpoint_content(
        path,
        PrintCheckpointContentOptions {
            digest: checkpoint.content_digest,
        },
    )
}

pub fn print_checkpoint_content(
    path: &Path,
    opt: PrintCheckpointContentOptions,
) -> anyhow::Result<()> {
    let checkpoint_store = CheckpointStore::new(&path.join("checkpoints"));
    let contents = checkpoint_store
        .get_checkpoint_contents(&opt.digest)?
        .ok_or(anyhow!(
            "Checkpoint content digest {:?} not found in checkpoint store",
            opt.digest
        ))?;
    println!("Checkpoint content: {:?}", contents);
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
    let perpetual_db = AuthorityPerpetualTables::open_tables_read_write(
        path.join("store").join("perpetual"),
        MetricConf::default(),
        None,
        None,
    );
    perpetual_db.reset_db_for_execution_since_genesis()?;

    let checkpoint_db = CheckpointStore::new(&path.join("checkpoints"));
    checkpoint_db.reset_db_for_execution_since_genesis()?;

    let epoch_db = AuthorityEpochTables::open_tables_read_write(
        path.join("store"),
        MetricConf::default(),
        None,
        None,
    );
    epoch_db.reset_db_for_execution_since_genesis()?;

    Ok(())
}

/// Force sets the highest executed checkpoint.
/// NOTE: Does not force re-execution of transactions.
/// Run with: cargo run --package sui-tool -- db-tool --db-path /opt/sui/db/authorities_db/live rewind-checkpoint-execution --epoch 3 --checkpoint-sequence-number 300000
pub fn rewind_checkpoint_execution(
    path: &Path,
    epoch: EpochId,
    checkpoint_sequence_number: u64,
) -> anyhow::Result<()> {
    let checkpoint_db = CheckpointStore::new(&path.join("checkpoints"));
    let Some(checkpoint) =
        checkpoint_db.get_checkpoint_by_sequence_number(checkpoint_sequence_number)?
    else {
        bail!("Checkpoint {checkpoint_sequence_number} not found!");
    };
    if epoch != checkpoint.epoch() {
        bail!(
            "Checkpoint {checkpoint_sequence_number} is in epoch {} not {epoch}!",
            checkpoint.epoch()
        );
    }
    let highest_executed_sequence_number = checkpoint_db
        .get_highest_executed_checkpoint_seq_number()?
        .unwrap_or_default();
    if checkpoint_sequence_number > highest_executed_sequence_number {
        bail!(
            "Must rewind checkpoint execution to be not later than highest executed ({} > {})!",
            checkpoint_sequence_number,
            highest_executed_sequence_number
        );
    }
    checkpoint_db.set_highest_executed_checkpoint_subtle(&checkpoint)?;
    Ok(())
}

pub fn print_db_table_summary(
    store: StoreName,
    epoch: Option<EpochId>,
    path: PathBuf,
    table_name: &str,
) -> anyhow::Result<()> {
    let summary = table_summary(store, epoch, path, table_name)?;
    let quantiles = [25, 50, 75, 90, 99];
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

/// Force sets state sync checkpoint watermarks.
/// Run with (for example):
/// cargo run --package sui-tool -- db-tool --db-path /opt/sui/db/authorities_db/live set_checkpoint_watermark --highest-synced 300000
pub fn set_checkpoint_watermark(
    path: &Path,
    options: SetCheckpointWatermarkOptions,
) -> anyhow::Result<()> {
    let checkpoint_db = CheckpointStore::new(&path.join("checkpoints"));

    if let Some(highest_verified) = options.highest_verified {
        let Some(checkpoint) = checkpoint_db.get_checkpoint_by_sequence_number(highest_verified)?
        else {
            bail!("Checkpoint {highest_verified} not found");
        };
        checkpoint_db.update_highest_verified_checkpoint(&checkpoint)?;
    }
    if let Some(highest_synced) = options.highest_synced {
        let Some(checkpoint) = checkpoint_db.get_checkpoint_by_sequence_number(highest_synced)?
        else {
            bail!("Checkpoint {highest_synced} not found");
        };
        checkpoint_db.update_highest_synced_checkpoint(&checkpoint)?;
    }
    Ok(())
}

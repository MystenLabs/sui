// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod archives;
mod snapshot;

use archives::ArchivalCheckpointInfo;
use clap::Parser;
use sui_pg_db::DbArgs;
use url::Url;

use crate::snapshot::SnapshotRestorer;

#[derive(Parser, Debug, Clone)]
#[clap(name = "sui-indexer-alt-restorer")]
pub struct Args {
    /// Restore from end of this epoch.
    #[clap(long, env = "START_EPOCH", required = true)]
    pub start_epoch: u64,

    /// Url of the endpoint to fetch snapshot files from,
    /// for example <https://formal-snapshot.mainnet.sui.io>
    #[clap(long, env = "ENDPOINT", required = true)]
    pub endpoint: String,

    /// Bucket to fetch snapshot files from.
    #[clap(long, env = "SNAPSHOT_BUCKET", required = true)]
    pub snapshot_bucket: String,

    /// Bucket to fetch archive files from.
    #[clap(long, env = "ARCHIVE_URL", required = true)]
    pub archive_url: String,

    /// Local directory to temporarily store snapshot files.
    #[clap(long, env = "SNAPSHOT_LOCAL_DIR", required = true)]
    pub snapshot_local_dir: String,

    /// Number of concurrent restore tasks to run.
    #[clap(long, env = "CONCURRENCY", default_value_t = 50)]
    pub concurrency: usize,

    /// The URL of the database to connect to.
    #[clap(
        long,
        env = "DATABASE_URL",
        default_value = "postgres://postgres:postgrespw@localhost:5432/sui_indexer_alt"
    )]
    pub database_url: Url,

    /// Database connection arguments from `sui-pg-db`.
    #[clap(flatten)]
    pub db_args: DbArgs,
}

pub async fn restore(args: &Args) -> anyhow::Result<()> {
    let archival_checkpoint_info =
        ArchivalCheckpointInfo::read_archival_checkpoint_info(args).await?;
    let mut snapshot_restorer =
        SnapshotRestorer::new(args, archival_checkpoint_info.next_checkpoint_after_epoch).await?;
    snapshot_restorer.restore().await?;
    Ok(())
}

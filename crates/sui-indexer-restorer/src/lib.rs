// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod archives;
mod snapshot;

use archives::read_archival_checkpoint_info;
use clap::Parser;

use crate::snapshot::SnapshotRestorer;

#[derive(Parser, Debug, Clone)]
#[clap(name = "sui-indexer-restorer")]
pub struct Args {
    #[clap(long, env = "START_EPOCH", required = true)]
    pub start_epoch: u64,

    #[clap(long, env = "ENDPOINT", required = true)]
    pub endpoint: String,

    #[clap(long, env = "SNAPSHOT_BUCKET", required = true)]
    pub snapshot_bucket: String,

    #[clap(long, env = "ARCHIVE_BUCKET", required = true)]
    pub archive_bucket: String,

    #[clap(long, env = "SNAPSHOT_LOCAL_DIR", required = true)]
    pub snapshot_local_dir: String,

    #[clap(long, env = "DATABASE_URL", required = true)]
    pub database_url: String,

    #[clap(long, env = "CONCURRENCY", default_value_t = 50)]
    pub concurrency: usize,
}

pub async fn restore(args: &Args) -> anyhow::Result<()> {
    let archival_checkpoint_info = read_archival_checkpoint_info(args).await?;
    let mut snapshot_restorer =
        SnapshotRestorer::new(args, archival_checkpoint_info.next_checkpoint_after_epoch).await?;
    snapshot_restorer.restore().await?;
    Ok(())
}

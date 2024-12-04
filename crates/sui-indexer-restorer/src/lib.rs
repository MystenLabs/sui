// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod snapshot;

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
}

pub async fn restore(args: &Args) -> anyhow::Result<()> {
    let mut snapshot_restorer = SnapshotRestorer::new(args).await?;
    snapshot_restorer.restore().await?;
    Ok(())
}



// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use anyhow::{bail, Context};
use diesel::QueryDsl;
use sui_indexer_alt_reader::pg_reader::{Connection, PgReader};
use sui_indexer_alt_schema::schema::kv_genesis;
use sui_types::digests::{ChainIdentifier, CheckpointDigest};
use tokio::time;
use tokio_util::sync::CancellationToken;
use tracing::warn;

/// Repeatedly try to fetch the chain identifier from the database at a given interval, eventually
/// returning it, or an error if the task was cancelled before it had a chance to complete.
/// If no database is available, returns a default ChainIdentifier immediately.
pub(crate) async fn task(
    pg_reader: &PgReader,
    interval: Duration,
    cancel: CancellationToken,
) -> anyhow::Result<ChainIdentifier> {
    // If no database is available, return default chain identifier
    if !pg_reader.has_database() {
        return Ok(ChainIdentifier::default());
    }

    let mut interval = time::interval(interval);

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                bail!("Shutdown signal received, terminating chain identifier task");
            }

            _ = interval.tick() => {
                let mut conn = match pg_reader.connect().await {
                    Ok(conn) => conn,
                    Err(e) => {
                        warn!("Failed to connect to database: {e}");
                        continue;
                    }
                };

                match fetch(&mut conn).await {
                    Ok(chain_identifier) => return Ok(chain_identifier),
                    Err(e) => {
                        warn!("Failed to fetch chain identifier: {e}");
                        continue;
                    }
                }
            }
        }
    }
}

/// Try to fetch the chain identifier from the database. Fails if the genesis checkpoint digest has
/// not been written to the database yet, or if it cannot be deserialized as a checkpoint digest.
async fn fetch(conn: &mut Connection<'_>) -> anyhow::Result<ChainIdentifier> {
    use kv_genesis::dsl as g;

    let digest_bytes: Vec<_> = conn
        .first(g::kv_genesis.select(g::genesis_digest))
        .await
        .context("Failed to fetch digest information")?;

    let digest =
        CheckpointDigest::try_from(digest_bytes).context("Failed to deserialize genesis digest")?;

    Ok(ChainIdentifier::from(digest))
}

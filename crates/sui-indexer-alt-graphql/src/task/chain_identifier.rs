// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{sync::Arc, time::Duration};

use anyhow::Context;
use diesel::QueryDsl;
use sui_futures::service::Service;
use sui_indexer_alt_reader::pg_reader::{Connection, PgReader};
use sui_indexer_alt_schema::schema::kv_genesis;
use sui_types::digests::{ChainIdentifier as NativeChainIdentifier, CheckpointDigest};
use tokio::{sync::SetOnce, time};
use tracing::warn;

#[derive(Clone, Default)]
pub(crate) struct ChainIdentifier(Arc<SetOnce<NativeChainIdentifier>>);

impl ChainIdentifier {
    /// Wait for the chain identifier to be set, and return it.
    pub(crate) async fn wait(&self) -> NativeChainIdentifier {
        *self.0.wait().await
    }

    // Set the chain identifier.
    fn set(&self, chain: NativeChainIdentifier) -> anyhow::Result<()> {
        self.0.set(chain).context("Chain Identifier already set")
    }
}

/// Repeatedly try to fetch the chain identifier from the database at a given interval, eventually
/// setting it in a `SetOnce` construct. If no database is available, returns a default
/// ChainIdentifier immediately.
///
/// Returns the container that is populated with the chain identifier once it has been fetched, and
/// a handle to the service performing the fetching.
pub(crate) fn task(pg_reader: PgReader, interval: Duration) -> (ChainIdentifier, Service) {
    let crx = ChainIdentifier::default();
    let ctx = crx.clone();

    let svc = Service::new().spawn_aborting(async move {
        if !pg_reader.has_database() {
            ctx.set(NativeChainIdentifier::default())
                .context("Failed to set ")
                .expect("SAFETY: Only this task sets the chain identifier, immediately before returning.");
            return Ok(());
        }

        let mut interval = time::interval(interval);

        loop {
            interval.tick().await;

            let mut conn = match pg_reader.connect().await {
                Ok(conn) => conn,
                Err(e) => {
                    warn!("Failed to connect to database: {e}");
                    continue;
                }
            };

            match fetch(&mut conn).await {
                Ok(chain) => {
                    ctx.set(chain)
                        .expect("SAFETY: Only this task sets the chain identifier, immediately before returning.");
                    return Ok(());
                }

                Err(e) => {
                    warn!("Failed to fetch chain identifier: {e}");
                    continue;
                }
            }
        }
    });

    (crx, svc)
}

/// Try to fetch the chain identifier from the database. Fails if the genesis checkpoint digest has
/// not been written to the database yet, or if it cannot be deserialized as a checkpoint digest.
async fn fetch(conn: &mut Connection<'_>) -> anyhow::Result<NativeChainIdentifier> {
    use kv_genesis::dsl as g;

    let digest_bytes: Vec<_> = conn
        .first(g::kv_genesis.select(g::genesis_digest))
        .await
        .context("Failed to fetch digest information")?;

    let digest =
        CheckpointDigest::try_from(digest_bytes).context("Failed to deserialize genesis digest")?;

    Ok(NativeChainIdentifier::from(digest))
}

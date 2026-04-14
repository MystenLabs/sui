// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use async_graphql::Context;
use sui_indexer_alt_reader::package_resolver::PackageCache;
use tokio::sync::broadcast;
use tracing::warn;

use crate::api::types::checkpoint::Checkpoint;
use crate::config::Limits;
use crate::error::RpcError;
use crate::scope::Scope;
use crate::task::streaming::CheckpointBroadcaster;

#[derive(Default)]
pub struct Subscription;

#[async_graphql::Subscription]
impl Subscription {
    /// Subscribe to checkpoints as they are finalized, starting from the current tip.
    ///
    /// This subscription is not yet available for use.
    async fn checkpoints(
        &self,
        ctx: &Context<'_>,
    ) -> Result<impl futures::Stream<Item = Result<Checkpoint, RpcError>>, RpcError> {
        let package_store = ctx.data::<Arc<PackageCache>>()?.clone();
        let limits: &Limits = ctx.data()?;
        let resolver_limits = limits.package_resolver();
        let broadcaster: &CheckpointBroadcaster = ctx.data()?;

        Ok(async_stream::stream! {
            let mut receiver = broadcaster.resubscribe();
            loop {
                match receiver.recv().await {
                    Ok(processed) => {
                        let scope = Scope::for_streamed_checkpoint(
                            package_store.clone(),
                            resolver_limits.clone(),
                        );
                        yield Ok(Checkpoint {
                            sequence_number: processed.sequence_number,
                            scope,
                            streamed_data: Some(processed),
                        });
                    }
                    Err(broadcast::error::RecvError::Lagged(missed_count)) => {
                        warn!(missed_count, "Checkpoint subscription lagged, disconnecting");
                        yield Err(anyhow::anyhow!(
                            "Subscription too slow: missed {missed_count} checkpoints. \
                             Please reconnect and use the query API to backfill \
                             from your last seen sequenceNumber."
                        ).into());
                        break;
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        warn!("Checkpoint broadcast channel closed");
                        yield Err(anyhow::anyhow!(
                            "Checkpoint stream has been shut down. Please reconnect."
                        ).into());
                        break;
                    }
                }
            }
        })
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use async_graphql::Context;
use tokio::sync::broadcast;
use tracing::warn;

use crate::api::types::checkpoint::Checkpoint;
use crate::api::types::event::Event;
use crate::api::types::event::filter::EventFilter;
use crate::api::types::transaction::Transaction;
use crate::api::types::transaction::TransactionContents;
use crate::api::types::transaction::filter::TransactionFilter;
use crate::config::Limits;
use crate::error::RpcError;
use crate::scope::Scope;
use crate::task::streaming::CheckpointBroadcaster;
use crate::task::streaming::StreamingPackageStore;

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
        let package_store = ctx.data::<Arc<StreamingPackageStore>>()?.clone();
        let limits: &Limits = ctx.data()?;
        let resolver_limits = limits.package_resolver();
        let mut receiver = ctx.data::<CheckpointBroadcaster>()?.resubscribe();

        Ok(async_stream::stream! {
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
                    Err(e) => {
                        yield Err(broadcast_error(e));
                        break;
                    }
                }
            }
        })
    }

    /// Subscribe to transactions as they are finalized, with optional filtering.
    ///
    /// Each matching transaction is yielded individually as it appears in finalized
    /// checkpoints. Transactions are ordered by checkpoint, then by position within
    /// the checkpoint.
    ///
    /// This subscription is not yet available for use.
    async fn transactions(
        &self,
        ctx: &Context<'_>,
        filter: Option<TransactionFilter>,
    ) -> Result<impl futures::Stream<Item = Result<Transaction, RpcError>>, RpcError> {
        let package_store = ctx.data::<Arc<StreamingPackageStore>>()?.clone();
        let limits: &Limits = ctx.data()?;
        let resolver_limits = limits.package_resolver();
        let mut receiver = ctx.data::<CheckpointBroadcaster>()?.resubscribe();
        let filter = filter.unwrap_or_default();

        Ok(async_stream::stream! {
            loop {
                match receiver.recv().await {
                    Ok(processed) => {
                        let scope = Scope::for_streamed_checkpoint(
                            package_store.clone(),
                            resolver_limits.clone(),
                        );
                        // TODO(DVX-2050): Pre-filter checkpoints using bloom filters
                        // before evaluating exact matches, to skip checkpoints with
                        // no matching transactions.
                        for tx in &processed.transactions {
                            if !filter.matches(&tx.contents) {
                                continue;
                            }
                            yield Ok(Transaction {
                                digest: tx.digest,
                                contents: TransactionContents {
                                    scope: scope.with_execution_objects(
                                        tx.execution_objects.clone(),
                                    ),
                                    contents: Some(Arc::new(tx.contents.clone())),
                                },
                            });
                        }
                    }
                    Err(e) => {
                        yield Err(broadcast_error(e));
                        break;
                    }
                }
            }
        })
    }

    /// Subscribe to events as they are emitted, with optional filtering.
    ///
    /// Each matching event is yielded individually as it appears in finalized
    /// checkpoints. Events are ordered by checkpoint, then by transaction
    /// position within the checkpoint, then by position within the transaction.
    ///
    /// This subscription is not yet available for use.
    async fn events(
        &self,
        ctx: &Context<'_>,
        filter: Option<EventFilter>,
    ) -> Result<impl futures::Stream<Item = Result<Event, RpcError>>, RpcError> {
        let package_store = ctx.data::<Arc<StreamingPackageStore>>()?.clone();
        let limits: &Limits = ctx.data()?;
        let resolver_limits = limits.package_resolver();
        let mut receiver = ctx.data::<CheckpointBroadcaster>()?.resubscribe();
        let filter = filter.unwrap_or_default();

        Ok(async_stream::stream! {
            loop {
                match receiver.recv().await {
                    Ok(processed) => {
                        let timestamp_ms = Some(processed.summary.timestamp_ms);
                        let scope = Scope::for_streamed_checkpoint(
                            package_store.clone(),
                            resolver_limits.clone(),
                        );
                        for tx in &processed.transactions {
                            let events = tx.contents.events().unwrap_or_default();
                            for (idx, native) in events.into_iter().enumerate() {
                                if !filter.matches(&native) {
                                    continue;
                                }
                                yield Ok(Event {
                                    scope: scope.with_execution_objects(
                                        tx.execution_objects.clone(),
                                    ),
                                    native,
                                    transaction_digest: tx.digest,
                                    sequence_number: idx as u64,
                                    timestamp_ms,
                                });
                            }
                        }
                    }
                    Err(e) => {
                        yield Err(broadcast_error(e));
                        break;
                    }
                }
            }
        })
    }
}

fn broadcast_error(e: broadcast::error::RecvError) -> RpcError {
    match e {
        broadcast::error::RecvError::Lagged(missed_count) => {
            warn!(missed_count, "Subscription lagged, disconnecting");
            anyhow::anyhow!(
                "Subscription too slow: missed {missed_count} checkpoints. \
                 Please reconnect and use the query API to backfill \
                 from your last seen sequenceNumber."
            )
            .into()
        }
        broadcast::error::RecvError::Closed => {
            warn!("Checkpoint broadcast channel closed");
            anyhow::anyhow!("Checkpoint stream has been shut down. Please reconnect.").into()
        }
    }
}

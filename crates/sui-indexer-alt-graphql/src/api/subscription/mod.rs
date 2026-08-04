// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use async_graphql::Context;
use async_graphql::connection::CursorType;
use async_graphql::connection::Edge;
use async_graphql::connection::EmptyFields;
use futures::StreamExt;
use sui_indexer_alt_reader::alpha_ledger_grpc_reader::AlphaLedgerGrpcReader;
use sui_indexer_alt_reader::ledger_grpc_reader::LedgerGrpcReader;
use tokio::sync::watch;

use crate::api::scalars::uint53::UInt53;
use crate::api::types::checkpoint::CCheckpoint;
use crate::api::types::checkpoint::Checkpoint;
use crate::api::types::checkpoint::CheckpointToken;
use crate::api::types::event::CEvent;
use crate::api::types::event::Event;
use crate::api::types::event::filter::EventFilter;
use crate::api::types::transaction::CTransaction;
use crate::api::types::transaction::Transaction;
use crate::api::types::transaction::filter::TransactionFilter;
use crate::config::Limits;
use crate::config::SubscriptionConfig;
use crate::error::RpcError;
use crate::error::bad_user_input;
use crate::scope::Scope;
use crate::task::streaming::StreamingPackageStore;
use crate::task::streaming::SubscriptionBroadcast;
use crate::task::watermark::Watermarks;

mod events;
mod scan_then_live;
mod transactions;

use scan_then_live::subscribe;

#[derive(thiserror::Error, Debug)]
pub(crate) enum Error {
    #[error("Filtering by `atCheckpoint` or `beforeCheckpoint` is not supported for subscriptions")]
    CheckpointBoundsUnsupported,
}

#[derive(Default)]
pub struct Subscription;

#[async_graphql::Subscription]
impl Subscription {
    /// Subscribe to checkpoints as they are finalized.
    ///
    /// Pass `after` (opaque cursor) or `afterCheckpoint` (sequence number) to resume from a known point. If both are provided, the subscription resumes from whichever is later.
    ///
    /// This subscription is not yet available for use.
    async fn checkpoints(
        &self,
        ctx: &Context<'_>,
        after: Option<CCheckpoint>,
        after_checkpoint: Option<UInt53>,
    ) -> Result<
        impl futures::Stream<Item = Result<Edge<String, Checkpoint, EmptyFields>, RpcError>>,
        RpcError,
    > {
        let package_store: &Arc<StreamingPackageStore> = ctx.data()?;
        let limits: &Limits = ctx.data()?;
        let config: &SubscriptionConfig = ctx.data()?;
        let broadcast: &Arc<SubscriptionBroadcast> = ctx.data()?;
        let fetcher: &LedgerGrpcReader = ctx.data()?;

        let resume_from: Option<u64> = match (
            after.map(|c| c.sequence_number()),
            after_checkpoint.map(u64::from),
        ) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        };
        let package_store = package_store.clone();
        let resolver_limits = limits.package_resolver();

        let stream = broadcast
            .clone()
            .subscribe(resume_from, fetcher.clone(), config);

        Ok(stream.map(move |item| {
            item.map(|processed| {
                let sequence_number = processed.summary.sequence_number;
                let scope = Scope::for_streamed_checkpoint(
                    package_store.clone(),
                    resolver_limits.clone(),
                    processed.clone(),
                );
                let cursor = CheckpointToken::cursor(sequence_number).encode_cursor();
                Edge::new(
                    cursor,
                    Checkpoint {
                        sequence_number,
                        scope,
                        streamed_data: Some(processed),
                    },
                )
            })
        }))
    }

    /// Subscribe to transactions as they are finalized, with optional filtering.
    ///
    /// Resume from a known point with `after` (an opaque cursor from a prior delivery) and/or `filter.afterCheckpoint`. The subscription first backfills the matching transactions after that point via the scanning API, then continues with the live stream.
    ///
    /// Each matching transaction is yielded individually as an edge, ordered by checkpoint and then by position within the checkpoint. Each edge carries a cursor for resumption.
    ///
    /// This subscription is not yet available for use.
    async fn transactions(
        &self,
        ctx: &Context<'_>,
        filter: Option<TransactionFilter>,
        after: Option<CTransaction>,
    ) -> Result<
        impl futures::Stream<Item = Result<Edge<String, Transaction, EmptyFields>, RpcError>>,
        RpcError<Error>,
    > {
        let package_store: &Arc<StreamingPackageStore> = ctx.data()?;
        let limits: &Limits = ctx.data()?;
        let config: &SubscriptionConfig = ctx.data()?;
        let broadcast: &Arc<SubscriptionBroadcast> = ctx.data()?;
        let reader: &AlphaLedgerGrpcReader = ctx.data()?;
        let watermarks_rx: &watch::Receiver<Arc<Watermarks>> = ctx.data()?;

        let package_store = package_store.clone();
        let resolver_limits = limits.package_resolver();
        let filter = filter.unwrap_or_default();

        // Size the backfill scan page to the resolve concurrency. Scans are sequential (each needs
        // the previous page's cursor), so feeding one window of `n` concurrent resolutions takes
        // ceil(n / page) scans: a page much smaller than the concurrency makes scanning the
        // bottleneck, a much larger one just holds a bigger page in memory. Matching them is roughly
        // one scan per resolution window.
        let scan_page_size = config.max_concurrent_resolutions;

        // Pin the handoff once the scan comes within half the live buffer of the tip, leaving room
        // for checkpoints that arrive during the handoff so the receiver does not lag.
        let handoff_threshold = config.broadcast_buffer as u64 / 2;

        if filter.at_checkpoint.is_some() || filter.before_checkpoint.is_some() {
            return Err(bad_user_input(Error::CheckpointBoundsUnsupported));
        }

        let after_checkpoint = filter.after_checkpoint.map(u64::from);

        Ok(subscribe::<Transaction>(
            reader.clone(),
            broadcast.clone(),
            package_store,
            resolver_limits,
            watermarks_rx.clone(),
            filter,
            after,
            after_checkpoint,
            scan_page_size,
            handoff_threshold,
        ))
    }

    /// Subscribe to events as they are emitted, with optional filtering.
    ///
    /// Resume from a known point with `after` (an opaque cursor from a prior delivery) and/or `filter.afterCheckpoint`. The subscription first backfills the matching events after that point via the scanning API, then continues with the live stream.
    ///
    /// Each matching event is yielded individually as an edge, ordered by checkpoint, then by transaction position within the checkpoint, then by position within the transaction. Each edge carries a cursor for resumption.
    ///
    /// This subscription is not yet available for use.
    async fn events(
        &self,
        ctx: &Context<'_>,
        filter: Option<EventFilter>,
        after: Option<CEvent>,
    ) -> Result<
        impl futures::Stream<Item = Result<Edge<String, Event, EmptyFields>, RpcError>>,
        RpcError<Error>,
    > {
        let package_store: &Arc<StreamingPackageStore> = ctx.data()?;
        let limits: &Limits = ctx.data()?;
        let config: &SubscriptionConfig = ctx.data()?;
        let broadcast: &Arc<SubscriptionBroadcast> = ctx.data()?;
        let reader: &AlphaLedgerGrpcReader = ctx.data()?;
        let watermarks_rx: &watch::Receiver<Arc<Watermarks>> = ctx.data()?;

        let package_store = package_store.clone();
        let resolver_limits = limits.package_resolver();
        let filter = filter.unwrap_or_default();

        // Size the backfill scan page to the resolve concurrency. Scans are sequential (each needs
        // the previous page's cursor), so feeding one window of `n` concurrent resolutions takes
        // ceil(n / page) scans: a page much smaller than the concurrency makes scanning the
        // bottleneck, a much larger one just holds a bigger page in memory. Matching them is roughly
        // one scan per resolution window.
        let scan_page_size = config.max_concurrent_resolutions;

        // Pin the handoff once the scan comes within half the live buffer of the tip, leaving room
        // for checkpoints that arrive during the handoff so the receiver does not lag.
        let handoff_threshold = config.broadcast_buffer as u64 / 2;

        if filter.at_checkpoint.is_some() || filter.before_checkpoint.is_some() {
            return Err(bad_user_input(Error::CheckpointBoundsUnsupported));
        }

        let after_checkpoint = filter.after_checkpoint.map(u64::from);

        Ok(subscribe::<Event>(
            reader.clone(),
            broadcast.clone(),
            package_store,
            resolver_limits,
            watermarks_rx.clone(),
            filter,
            after,
            after_checkpoint,
            scan_page_size,
            handoff_threshold,
        ))
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use async_graphql::Context;
use async_graphql::SimpleObject;
use async_graphql::connection::Edge;
use async_graphql::connection::EmptyFields;
use futures::Stream;
use futures::StreamExt;
use sui_indexer_alt_reader::alpha_ledger_grpc_reader::AlphaLedgerGrpcReader;
use tokio::sync::watch;

use crate::api::scalars::uint53::UInt53;
use crate::api::types::checkpoint::CCheckpoint;
use crate::api::types::checkpoint::Checkpoint;
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
use crate::task::streaming::StreamedCaches;
use crate::task::streaming::SubscriberLimit;
use crate::task::streaming::SubscriptionBroadcast;
use crate::task::watermark::Watermarks;

mod checkpoints;
mod events;
mod scan_then_live;
mod transactions;

use scan_then_live::Delivery;
use scan_then_live::subscribe;

#[derive(thiserror::Error, Debug)]
pub(crate) enum Error {
    #[error("Filtering by `atCheckpoint` or `beforeCheckpoint` is not supported for subscriptions")]
    CheckpointBoundsUnsupported,

    #[error("Cannot start a subscription more than {max} checkpoints ahead of the current tip")]
    TooFarAheadOfTip { max: u64 },
}

#[derive(Default)]
pub struct Subscription;

/// One payload of the `transactions` subscription: a matching transaction, or progress.
#[derive(SimpleObject)]
pub(crate) struct TransactionSubscriptionEdge {
    /// Pass as `after` to resume the subscription after this payload.
    cursor: String,

    /// The matching transaction, or `null` for progress.
    ///
    /// A payload whose `node` is `null` and that has no `errors` is progress: every match before its cursor has been delivered, so save the cursor and resume `after` it to skip the range already scanned.
    ///
    /// A payload whose `node` is `null` and that has `errors` is a match that failed to resolve. Do not save its cursor: treat the payload as an error and resume from the previous cursor.
    node: Option<Transaction>,
}

/// One payload of the `events` subscription: a matching event, or progress.
#[derive(SimpleObject)]
pub(crate) struct EventSubscriptionEdge {
    /// Pass as `after` to resume the subscription after this payload.
    cursor: String,

    /// The matching event, or `null` for progress.
    ///
    /// A payload whose `node` is `null` and that has no `errors` is progress: every match before its cursor has been delivered, so save the cursor and resume `after` it to skip the range already scanned.
    ///
    /// A payload whose `node` is `null` and that has `errors` is a match that failed to resolve. Do not save its cursor: treat the payload as an error and resume from the previous cursor.
    node: Option<Event>,
}

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
        RpcError<Error>,
    > {
        let caches: &Arc<StreamedCaches> = ctx.data()?;
        let limits: &Limits = ctx.data()?;
        let config: &SubscriptionConfig = ctx.data()?;
        let broadcast: &Arc<SubscriptionBroadcast> = ctx.data()?;
        let reader: &AlphaLedgerGrpcReader = ctx.data()?;
        let watermarks_rx: &watch::Receiver<Arc<Watermarks>> = ctx.data()?;
        let subscriber_limit: &SubscriberLimit = ctx.data()?;

        // The checkpoints feed sends no progress (every checkpoint is a match), so every delivery
        // is a match and `CheckpointEdge`'s node stays non-null.
        let stream = subscribe::<Checkpoint>(
            reader.clone(),
            broadcast.clone(),
            caches.clone(),
            limits.package_resolver(),
            watermarks_rx.clone(),
            (),
            after,
            after_checkpoint.map(u64::from),
            subscriber_limit.clone(),
            config.clone(),
        )?;
        Ok(stream.filter_map(|delivery| {
            futures::future::ready(delivery.map(Delivery::into_match).transpose())
        }))
    }

    /// Subscribe to transactions as they are finalized, with optional filtering.
    ///
    /// Resume from a known point with `after` (an opaque cursor from a prior delivery) and/or `filter.afterCheckpoint`. The subscription first backfills the matching transactions after that point via the scanning API, then continues with the live stream.
    ///
    /// Each matching transaction is yielded individually as an edge, ordered by checkpoint and then by position within the checkpoint. Each edge carries a cursor for resumption.
    ///
    /// A payload whose `node` is `null` and that has no `errors` reports progress: every match before its cursor has been delivered, so saving its cursor and resuming `after` it skips the range scanned so far without missing anything. Progress is sent at the end of each backfill scan page that ends past its last match, and during the live stream after a run of checkpoints with no match.
    ///
    /// A payload whose `node` is `null` and that has `errors` is a match that failed to resolve, not progress. Do not save its cursor: treat the payload as an error and resume from the previous cursor.
    ///
    /// This subscription is not yet available for use.
    async fn transactions(
        &self,
        ctx: &Context<'_>,
        filter: Option<TransactionFilter>,
        after: Option<CTransaction>,
    ) -> Result<impl Stream<Item = Result<TransactionSubscriptionEdge, RpcError>>, RpcError<Error>>
    {
        let caches: &Arc<StreamedCaches> = ctx.data()?;
        let limits: &Limits = ctx.data()?;
        let config: &SubscriptionConfig = ctx.data()?;
        let broadcast: &Arc<SubscriptionBroadcast> = ctx.data()?;
        let reader: &AlphaLedgerGrpcReader = ctx.data()?;
        let watermarks_rx: &watch::Receiver<Arc<Watermarks>> = ctx.data()?;
        let subscriber_limit: &SubscriberLimit = ctx.data()?;

        let filter = filter.unwrap_or_default();
        if filter.at_checkpoint.is_some() || filter.before_checkpoint.is_some() {
            return Err(bad_user_input(Error::CheckpointBoundsUnsupported));
        }
        let after_checkpoint = filter.after_checkpoint.map(u64::from);

        let stream = subscribe::<Transaction>(
            reader.clone(),
            broadcast.clone(),
            caches.clone(),
            limits.package_resolver(),
            watermarks_rx.clone(),
            filter,
            after,
            after_checkpoint,
            subscriber_limit.clone(),
            config.clone(),
        )?;
        Ok(stream.map(|delivery| {
            delivery.map(|d| {
                let (cursor, node) = d.into_parts();
                TransactionSubscriptionEdge { cursor, node }
            })
        }))
    }

    /// Subscribe to events as they are emitted, with optional filtering.
    ///
    /// Resume from a known point with `after` (an opaque cursor from a prior delivery) and/or `filter.afterCheckpoint`. The subscription first backfills the matching events after that point via the scanning API, then continues with the live stream.
    ///
    /// Each matching event is yielded individually as an edge, ordered by checkpoint, then by transaction position within the checkpoint, then by position within the transaction. Each edge carries a cursor for resumption.
    ///
    /// A payload whose `node` is `null` and that has no `errors` reports progress: every match before its cursor has been delivered, so saving its cursor and resuming `after` it skips the range scanned so far without missing anything. Progress is sent at the end of each backfill scan page that ends past its last match, and during the live stream after a run of checkpoints with no match.
    ///
    /// A payload whose `node` is `null` and that has `errors` is a match that failed to resolve, not progress. Do not save its cursor: treat the payload as an error and resume from the previous cursor.
    ///
    /// This subscription is not yet available for use.
    async fn events(
        &self,
        ctx: &Context<'_>,
        filter: Option<EventFilter>,
        after: Option<CEvent>,
    ) -> Result<impl Stream<Item = Result<EventSubscriptionEdge, RpcError>>, RpcError<Error>> {
        let caches: &Arc<StreamedCaches> = ctx.data()?;
        let limits: &Limits = ctx.data()?;
        let config: &SubscriptionConfig = ctx.data()?;
        let broadcast: &Arc<SubscriptionBroadcast> = ctx.data()?;
        let reader: &AlphaLedgerGrpcReader = ctx.data()?;
        let watermarks_rx: &watch::Receiver<Arc<Watermarks>> = ctx.data()?;
        let subscriber_limit: &SubscriberLimit = ctx.data()?;

        let filter = filter.unwrap_or_default();
        if filter.at_checkpoint.is_some() || filter.before_checkpoint.is_some() {
            return Err(bad_user_input(Error::CheckpointBoundsUnsupported));
        }
        let after_checkpoint = filter.after_checkpoint.map(u64::from);

        let stream = subscribe::<Event>(
            reader.clone(),
            broadcast.clone(),
            caches.clone(),
            limits.package_resolver(),
            watermarks_rx.clone(),
            filter,
            after,
            after_checkpoint,
            subscriber_limit.clone(),
            config.clone(),
        )?;
        Ok(stream.map(|delivery| {
            delivery.map(|d| {
                let (cursor, node) = d.into_parts();
                EventSubscriptionEdge { cursor, node }
            })
        }))
    }
}

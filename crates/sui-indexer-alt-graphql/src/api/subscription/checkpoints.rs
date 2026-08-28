// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Checkpoints as a scan-then-live feed: every checkpoint is delivered as its own edge, in
//! sequence-number order, unfiltered. The cursor is the checkpoint's sequence number. Backfill is
//! one unfiltered `ListCheckpoints` scan (a sequential read), and both backfilled and live nodes
//! carry their fully-processed checkpoint in memory, resolving fields from it without further reads.

use std::future::Future;
use std::ops::RangeInclusive;
use std::sync::Arc;

use anyhow::Context as _;
use async_graphql::connection::CursorType;
use async_graphql::connection::Edge;
use async_graphql::connection::EmptyFields;
use sui_indexer_alt_reader::alpha_ledger_grpc_reader::AlphaLedgerGrpcReader;
use sui_indexer_alt_reader::alpha_ledger_grpc_reader::StreamPage;
use sui_rpc::proto::sui::rpc::v2;
use sui_rpc_cursor::CursorToken;

use crate::api::types::checkpoint::CCheckpoint;
use crate::api::types::checkpoint::Checkpoint;
use crate::api::types::checkpoint::CheckpointToken;
use crate::error::RpcError;
use crate::pagination::Page;
use crate::scope::Scope;
use crate::task::streaming::ProcessedCheckpoint;
use crate::task::streaming::StreamedCaches;
use crate::task::streaming::checkpoint_field_mask;
use crate::task::streaming::process_checkpoint;

use super::scan_then_live::Subscribable;

impl Subscribable for Checkpoint {
    type Item = Self;
    type Cursor = CCheckpoint;
    /// Checkpoints are delivered unfiltered.
    type Filter = ();
    type ScanItem = v2::Checkpoint;

    fn subscription_type() -> &'static str {
        "checkpoints"
    }

    fn scan<'a>(
        reader: &'a AlphaLedgerGrpcReader,
        cp_bounds: RangeInclusive<u64>,
        page: &'a Page<CCheckpoint>,
        _filter: &'a (),
    ) -> impl Future<Output = Result<StreamPage<v2::Checkpoint>, RpcError>> + Send + 'a {
        list_checkpoints_page(reader, cp_bounds, page)
    }

    /// Fully process the scanned checkpoint and carry it on the node, resolving against it in memory.
    /// The scan supplied the data cheaply as one sequential read; processing it here is the same work
    /// the live path does off the broadcast, so a backfilled node resolves identically to a live one.
    fn build_node(
        caches: &Arc<StreamedCaches>,
        resolver_limits: &sui_package_resolver::Limits,
        payload: &v2::Checkpoint,
    ) -> Result<Self, RpcError> {
        let processed = Arc::new(process_checkpoint(payload.clone())?);
        let scope = Scope::for_streamed_checkpoint(
            caches.clone(),
            resolver_limits.clone(),
            processed.clone(),
        );
        Ok(Checkpoint {
            sequence_number: processed.summary.sequence_number,
            scope,
            streamed_data: Some(processed),
        })
    }

    /// A live checkpoint is delivered whole, as a single edge carrying its processed data.
    fn matching_edges(
        checkpoint: &Arc<ProcessedCheckpoint>,
        caches: &Arc<StreamedCaches>,
        resolver_limits: &sui_package_resolver::Limits,
        _filter: &(),
    ) -> Result<Vec<Edge<String, Self, EmptyFields>>, RpcError> {
        let scope = Scope::for_streamed_checkpoint(
            caches.clone(),
            resolver_limits.clone(),
            checkpoint.clone(),
        );
        let sequence_number = checkpoint.summary.sequence_number;
        let cursor = CheckpointToken::cursor(sequence_number).encode_cursor();
        let node = Checkpoint {
            sequence_number,
            scope,
            streamed_data: Some(checkpoint.clone()),
        };
        Ok(vec![Edge::new(cursor, node)])
    }
}

/// Scan one page of checkpoints over `cp_bounds`, resuming after `page`'s cursor. Unfiltered, so the
/// server does one sequential scan rather than an inverted-index lookup.
async fn list_checkpoints_page(
    reader: &AlphaLedgerGrpcReader,
    cp_bounds: RangeInclusive<u64>,
    page: &Page<CCheckpoint>,
) -> Result<StreamPage<v2::Checkpoint>, RpcError> {
    let after = page.after().map(|c| CursorToken::from(&c.token()).encode());
    let before = page
        .before()
        .map(|c| CursorToken::from(&c.token()).encode());

    let mut options = v2::QueryOptions::default();
    options.limit = Some(page.limit() as u32);
    options.after = after;
    options.before = before;
    options.ordering = Some(if page.is_from_front() {
        v2::Ordering::Ascending as i32
    } else {
        v2::Ordering::Descending as i32
    });

    let mut request = v2::ListCheckpointsRequest::default();
    request.read_mask = Some(checkpoint_field_mask());
    request.start_checkpoint = Some(*cp_bounds.start());
    request.end_checkpoint = Some(cp_bounds.end().saturating_add(1));
    request.filter = None;
    request.options = Some(options);

    Ok(reader
        .list_checkpoints(request)
        .await
        .context("Failed to list checkpoints")?)
}

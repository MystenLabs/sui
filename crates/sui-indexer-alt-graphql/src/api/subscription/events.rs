// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Events as a scan-then-live feed: each matching event is delivered as its own edge, ordered by
//! checkpoint, then by transaction position within the checkpoint, then by position within the
//! transaction. The cursor places the event by those three indices, and matching tests each event
//! against the filter.

use std::future::Future;
use std::ops::RangeInclusive;
use std::sync::Arc;

use sui_indexer_alt_reader::ledger_grpc_reader::LedgerGrpcReader;
use sui_rpc::proto::sui::rpc::v2;
use tokio::sync::OnceCell;

use crate::api::types::event::CEvent;
use crate::api::types::event::Event;
use crate::api::types::event::EventToken;
use crate::api::types::event::event_from_stream_item;
use crate::api::types::event::filter::EventFilter;
use crate::api::types::ledger_cursor::EventKey;
use crate::api::types::ledger_cursor::LedgerKey;
use crate::api::types::ledger_cursor::scan::LedgerPage;
use crate::error::RpcError;
use crate::pagination::Page;
use crate::scope::Scope;
use crate::task::streaming::ProcessedCheckpoint;
use crate::task::streaming::StreamedCaches;

use super::EventSubscriptionEdge;
use super::scan_then_live::Subscribable;

impl Subscribable for Event {
    type Item = Self;
    type Cursor = CEvent;
    type Position = EventKey;
    type Filter = EventFilter;
    type ScanItem = v2::Event;
    type Output = EventSubscriptionEdge;

    fn subscription_type() -> &'static str {
        "events"
    }

    fn output(cursor: String, node: Option<Event>) -> EventSubscriptionEdge {
        EventSubscriptionEdge { cursor, node }
    }

    fn scan<'a>(
        reader: &'a LedgerGrpcReader,
        cp_bounds: RangeInclusive<u64>,
        page: &'a Page<CEvent>,
        filter: &'a EventFilter,
    ) -> impl Future<Output = Result<LedgerPage<v2::Event, CEvent>, RpcError>> + Send + 'a {
        Event::scan_grpc(reader, cp_bounds, page, page.limit(), filter)
    }

    fn resume_from(cursor: &CEvent) -> Option<u64> {
        cursor.point().last_checkpoint()
    }

    fn resume_position(cursor: &CEvent) -> EventKey {
        cursor.point().gap_after().key()
    }

    fn checkpoint_start(checkpoint: u64) -> EventKey {
        EventKey::checkpoint_start(checkpoint)
    }

    fn build_node(
        caches: &Arc<StreamedCaches>,
        resolver_limits: &sui_package_resolver::Limits,
        checkpoint: u64,
        payload: &v2::Event,
    ) -> Result<Self, RpcError> {
        let scope = Scope::for_backfill(caches.clone(), resolver_limits.clone(), checkpoint);
        event_from_stream_item(scope, payload)
    }

    fn matching_items(
        checkpoint: &Arc<ProcessedCheckpoint>,
        caches: &Arc<StreamedCaches>,
        resolver_limits: &sui_package_resolver::Limits,
        filter: &EventFilter,
    ) -> Result<Vec<(CEvent, Self)>, RpcError> {
        let timestamp_ms = Some(checkpoint.summary.timestamp_ms);
        let scope = Scope::for_streamed_checkpoint(
            caches.clone(),
            resolver_limits.clone(),
            checkpoint.clone(),
        );
        let mut items = Vec::new();
        // `transactions` holds every transaction in the checkpoint, in order.
        for (transaction_index, tx) in checkpoint.transactions.iter().enumerate() {
            let digest = tx
                .contents
                .digest()
                .expect("ExecutedTransaction digest is infallible");
            let events = tx.contents.events().unwrap_or_default();
            for (idx, native) in events.into_iter().enumerate() {
                if !filter.matches(&native) {
                    continue;
                }
                let cursor = EventToken::at(EventKey {
                    checkpoint: checkpoint.summary.sequence_number,
                    transaction_index: transaction_index as u64,
                    event_index: idx as u32,
                });
                items.push((
                    cursor,
                    Event {
                        scope: scope.with_active_transaction_contents(digest, tx.contents.clone()),
                        native,
                        transaction_digest: digest,
                        sequence_number: idx as u64,
                        timestamp_ms: OnceCell::from(timestamp_ms),
                    },
                ));
            }
        }
        Ok(items)
    }

    fn boundary_after(checkpoint: &ProcessedCheckpoint) -> Option<CEvent> {
        Some(EventToken::checkpoint_start(
            checkpoint.summary.sequence_number + 1,
        ))
    }
}

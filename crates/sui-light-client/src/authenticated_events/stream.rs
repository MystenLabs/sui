// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::mmr::apply_stream_updates;
use super::{AuthenticatedEvent, AuthenticatedEventsClient, ClientConfig, ClientError};
use futures::StreamExt;
use futures::stream::Stream;
use mysten_common::debug_fatal;
use std::sync::Arc;
use sui_rpc::field::{FieldMask, FieldMaskUtil};
use sui_rpc::proto::sui::rpc::v2alpha::ledger_service_client::LedgerServiceClient as V2AlphaLedgerServiceClient;
use sui_rpc::proto::sui::rpc::v2alpha::{
    AffectedObjectFilter, EventFilter, EventLiteral, EventPredicate, EventStreamHeadFilter,
    EventTerm, ListEventsRequest, ListTransactionsRequest, QueryEndReason, QueryOptions,
    TransactionFilter, TransactionLiteral, TransactionPredicate, TransactionTerm,
    list_events_response, list_transactions_response,
};
use sui_types::accumulator_root::{EventCommitment, EventStreamHead};
use sui_types::base_types::{ObjectID, SuiAddress};
use tokio::sync::mpsc;
use tonic::transport::Channel;

struct EventStreamState {
    client: Arc<AuthenticatedEventsClient>,
    stream_object_id: ObjectID,
    /// Inclusive lower bound on the next ListEvents request. Bumped only
    /// when the server reports a final `QueryEndReason` (LedgerTip /
    /// CheckpointBound / CursorBound / Unspecified) so the next request
    /// starts past the last fully-scanned checkpoint.
    next_checkpoint: u64,
    /// Opaque resume cursor within the current scan. Set when the server
    /// returns `ItemLimit` / `ScanLimit`, cleared on a final end reason so
    /// fresh events at the tip can be picked up by the next request.
    next_cursor: Option<Vec<u8>>,
    /// Events received from prior pages whose containing checkpoint may
    /// still have more events. They're buffered here until we know the
    /// checkpoint is complete (either the next page crosses into a later
    /// checkpoint or the scan ends), and only then folded into the MMR.
    /// Folding mid-checkpoint would compute a partial Merkle root that
    /// never matches the on-chain head.
    unverified_buffer: Vec<AuthenticatedEvent>,
    config: ClientConfig,
    last_verified_checkpoint: u64,
    verified_stream_head: Option<EventStreamHead>,
    filter: EventFilter,
}

impl EventStreamState {
    fn new(
        client: Arc<AuthenticatedEventsClient>,
        stream_id: SuiAddress,
        stream_object_id: ObjectID,
        start_checkpoint: u64,
        initial_stream_head: Option<EventStreamHead>,
        config: ClientConfig,
    ) -> Self {
        let filter = build_event_stream_head_filter(stream_id);
        Self {
            client,
            stream_object_id,
            next_checkpoint: start_checkpoint,
            next_cursor: None,
            unverified_buffer: Vec::new(),
            config,
            last_verified_checkpoint: start_checkpoint.saturating_sub(1),
            verified_stream_head: initial_stream_head,
            filter,
        }
    }

    async fn fetch_and_verify_next_batch(
        &mut self,
    ) -> Result<Vec<AuthenticatedEvent>, ClientError> {
        let mut new_events: Vec<AuthenticatedEvent> = Vec::new();
        let start_checkpoint = self.next_checkpoint;

        let mut options = QueryOptions::default().with_limit(self.config.page_size);
        if let Some(cursor) = self.next_cursor.as_ref() {
            options.set_after(cursor.clone());
        }
        // Request every field the in-tree `AuthenticatedEvent` converter needs:
        // the event body plus its ledger-position fields (`checkpoint`,
        // `transaction_index`, `event_index`), which the list endpoint only
        // populates when the read mask asks for them.
        let read_mask = FieldMask::from_paths([
            "package_id",
            "module",
            "sender",
            "event_type",
            "contents",
            "checkpoint",
            "transaction_index",
            "event_index",
        ]);
        let request = ListEventsRequest::default()
            .with_read_mask(read_mask)
            .with_start_checkpoint(start_checkpoint)
            .with_filter(self.filter.clone())
            .with_options(options);

        let mut ledger_service = self.client.ledger_service_v2alpha();
        let response = ledger_service
            .list_events(request)
            .await
            .map_err(ClientError::RpcError)?;
        let mut stream = response.into_inner();

        let mut last_cursor: Option<Vec<u8>> = None;
        let mut end_reason: Option<QueryEndReason> = None;

        while let Some(frame) = stream.next().await {
            let frame = frame.map_err(ClientError::RpcError)?;
            match frame.response {
                Some(list_events_response::Response::Item(item)) => {
                    if let Some(cursor) = item.watermark.as_ref().and_then(|w| w.cursor.as_ref()) {
                        last_cursor = Some(cursor.to_vec());
                    }
                    new_events.push(item.try_into()?);
                }
                Some(list_events_response::Response::Watermark(w)) => {
                    if let Some(cursor) = w.cursor.as_ref() {
                        last_cursor = Some(cursor.to_vec());
                    }
                }
                Some(list_events_response::Response::End(end)) => {
                    end_reason = Some(end.reason());
                }
                Some(_) | None => {}
            }
        }

        self.unverified_buffer.append(&mut new_events);

        // Decide which buffered events are ready to be folded into the MMR.
        // The on-chain MMR is folded one Merkle tree per checkpoint, so a
        // partial-checkpoint fold would never match the chain head. When
        // the scan is incomplete (`ItemLimit` / `ScanLimit`) we can only
        // commit events at checkpoints strictly before the most recent
        // one — the last checkpoint may still have more events on the next
        // page. A final end reason guarantees every event at the last
        // checkpoint has been emitted, so the entire buffer can flush.
        let scan_incomplete = matches!(
            end_reason,
            Some(QueryEndReason::ItemLimit) | Some(QueryEndReason::ScanLimit),
        );
        let flushed = self.drain_complete_checkpoints(scan_incomplete);

        // Resume the scan via cursor while it's still active. Once the
        // server signals a final end, drop the cursor so the next request
        // picks up events past the indexed tip.
        if scan_incomplete {
            self.next_cursor = last_cursor;
        } else {
            self.next_cursor = None;
        }

        // Advance the open lower bound of the canonical range only when a
        // scan completed — otherwise the buffered (or in-flight) events at
        // `start_checkpoint` are still being scanned.
        if !scan_incomplete {
            let advance_from = flushed
                .last()
                .map(|event| event.checkpoint)
                .or_else(|| self.unverified_buffer.last().map(|event| event.checkpoint));
            if let Some(cp) = advance_from {
                self.next_checkpoint = cp.saturating_add(1);
            }
        }

        if flushed.is_empty() {
            return Ok(Vec::new());
        }

        let last_flushed_checkpoint = flushed
            .last()
            .map(|event| event.checkpoint)
            .expect("non-empty checked above");
        if last_flushed_checkpoint > self.last_verified_checkpoint {
            self.verify_events(last_flushed_checkpoint, &flushed)
                .await?;
        }

        Ok(flushed)
    }

    /// Drain events from `unverified_buffer` whose checkpoint is known to
    /// be complete. When the surrounding scan is incomplete, hold back
    /// events at the buffer's last checkpoint (since the next page may
    /// still emit more events for it). When the scan ended cleanly, every
    /// buffered event is committable.
    fn drain_complete_checkpoints(&mut self, scan_incomplete: bool) -> Vec<AuthenticatedEvent> {
        if self.unverified_buffer.is_empty() {
            return Vec::new();
        }
        if !scan_incomplete {
            return std::mem::take(&mut self.unverified_buffer);
        }
        let last_checkpoint = self
            .unverified_buffer
            .last()
            .map(|event| event.checkpoint)
            .expect("non-empty checked above");
        let split_at = self
            .unverified_buffer
            .iter()
            .rposition(|event| event.checkpoint != last_checkpoint);
        match split_at {
            Some(idx) => self.unverified_buffer.drain(..=idx).collect(),
            None => Vec::new(),
        }
    }

    async fn verify_events(
        &mut self,
        up_to_checkpoint: u64,
        events: &[AuthenticatedEvent],
    ) -> Result<(), ClientError> {
        if up_to_checkpoint <= self.last_verified_checkpoint {
            return Ok(());
        }

        tracing::info!(
            "Verifying {} events from checkpoint {} to {}",
            events.len(),
            self.last_verified_checkpoint,
            up_to_checkpoint
        );

        let default_head = EventStreamHead::default();
        let old_head = self.verified_stream_head.as_ref().unwrap_or(&default_head);

        let (start_cp, end_cp) = match (events.first(), events.last()) {
            (Some(first), Some(last)) => (first.checkpoint, last.checkpoint),
            _ => (up_to_checkpoint, up_to_checkpoint),
        };
        let mut ledger_service = self.client.ledger_service_v2alpha();
        let settlements = fetch_settlements_for_range(
            &mut ledger_service,
            self.stream_object_id,
            start_cp,
            end_cp.saturating_add(1),
        )
        .await?;

        let accumulated_events =
            Self::bucket_events_by_settlement(events, &settlements, self.last_verified_checkpoint)?;

        let new_stream_head = self
            .client
            .fetch_and_verify_stream_head(self.stream_object_id, up_to_checkpoint)
            .await?;

        if new_stream_head.checkpoint_seq <= old_head.checkpoint_seq {
            return Err(ClientError::VerificationError(format!(
                "MMR verification failed: checkpoint went backwards from {} to {}",
                old_head.checkpoint_seq, new_stream_head.checkpoint_seq
            )));
        }

        let computed_head = apply_stream_updates(old_head, accumulated_events);

        if computed_head.mmr != new_stream_head.mmr {
            return Err(ClientError::VerificationError(
                "MMR verification failed: computed MMR root does not match EventStreamHead"
                    .to_string(),
            ));
        }

        if computed_head.num_events != new_stream_head.num_events {
            return Err(ClientError::VerificationError(format!(
                "MMR verification failed: computed event count {} does not match EventStreamHead count {}",
                computed_head.num_events, new_stream_head.num_events
            )));
        }

        if new_stream_head.checkpoint_seq != up_to_checkpoint {
            return Err(ClientError::VerificationError(format!(
                "MMR verification failed: stream head checkpoint {} does not match expected checkpoint {}",
                new_stream_head.checkpoint_seq, up_to_checkpoint
            )));
        }

        self.verified_stream_head = Some(new_stream_head);
        self.last_verified_checkpoint = up_to_checkpoint;

        Ok(())
    }

    /// Bucket events into per-settlement MMR-fold batches.
    ///
    /// The framework runs one `settle_events` (and one `add_to_mmr` fold)
    /// per consensus commit per stream, so a checkpoint that aggregates
    /// multiple commits has multiple folds at the same `checkpoint_seq`.
    /// Each event belongs to the next settlement transaction that follows
    /// it within the same checkpoint — i.e. the smallest
    /// `(checkpoint, tx_offset)` settlement boundary with
    /// `checkpoint == event.checkpoint` and `tx_offset >= event.transaction_offset`.
    ///
    /// Events sharing a settlement key form one batch (one merkle root,
    /// one MMR fold). Both `events` and `settlements` must be sorted in
    /// `(checkpoint, tx_offset)` order; this walks them with a single
    /// pointer and errors out if any event has no covering settlement
    /// (which would mean we under-fetched the settlement range).
    fn bucket_events_by_settlement(
        events: &[AuthenticatedEvent],
        settlements: &[(u64, u64)],
        last_verified_checkpoint: u64,
    ) -> Result<Vec<Vec<EventCommitment>>, ClientError> {
        let mut accumulated_events: Vec<Vec<EventCommitment>> = Vec::new();
        let mut current_key: Option<(u64, u64)> = None;
        let mut current_batch: Vec<EventCommitment> = Vec::new();
        let mut settlement_idx: usize = 0;

        for event in events {
            if event.checkpoint <= last_verified_checkpoint {
                debug_fatal!(
                    "Received event from checkpoint {} which is <= last_verified_checkpoint {}",
                    event.checkpoint,
                    last_verified_checkpoint
                );
                continue;
            }

            // Advance the settlement cursor past any boundary strictly
            // before the event in `(cp, tx_offset)` order. Equal-offset
            // matches stay because the settlement transaction at offset S
            // covers the user events with `tx_offset <= S`.
            while settlement_idx < settlements.len()
                && (settlements[settlement_idx].0 < event.checkpoint
                    || (settlements[settlement_idx].0 == event.checkpoint
                        && settlements[settlement_idx].1 < event.transaction_offset))
            {
                settlement_idx += 1;
            }

            if settlement_idx >= settlements.len()
                || settlements[settlement_idx].0 != event.checkpoint
            {
                return Err(ClientError::VerificationError(format!(
                    "no settlement transaction found covering event at \
                     (checkpoint {}, tx_offset {})",
                    event.checkpoint, event.transaction_offset
                )));
            }

            let settlement_key = settlements[settlement_idx];
            let commitment = EventCommitment::new(
                event.checkpoint,
                event.transaction_offset,
                event.event_index as u64,
                event.event.digest(),
            );

            match current_key {
                Some(key) if key == settlement_key => {
                    current_batch.push(commitment);
                }
                _ => {
                    if !current_batch.is_empty() {
                        accumulated_events.push(std::mem::take(&mut current_batch));
                    }
                    current_batch.push(commitment);
                    current_key = Some(settlement_key);
                }
            }
        }

        if !current_batch.is_empty() {
            accumulated_events.push(current_batch);
        }

        Ok(accumulated_events)
    }
}

/// List every settle_events transaction that touched the given
/// `EventStreamHead` object in `[start_checkpoint, end_checkpoint)`. Each
/// settle_events call mutates the stream head, so filtering
/// `ListTransactions` by `affected_object = event_stream_head_object_id`
/// returns exactly the per-stream settlement boundaries.
///
/// Returns a vector of `(checkpoint, transaction_offset)` sorted
/// ascending — the same order the events stream uses, which lets
/// downstream bucketing walk both with a single cursor.
async fn fetch_settlements_for_range(
    client: &mut V2AlphaLedgerServiceClient<Channel>,
    stream_object_id: ObjectID,
    start_checkpoint: u64,
    end_checkpoint_exclusive: u64,
) -> Result<Vec<(u64, u64)>, ClientError> {
    let filter = build_affected_object_filter(stream_object_id);
    // We need `checkpoint` and the `transaction_index` position field from each
    // settlement transaction.
    let read_mask = FieldMask::from_paths(["checkpoint", "transaction_index"]);

    let mut all: Vec<(u64, u64)> = Vec::new();
    let mut cursor: Option<Vec<u8>> = None;

    loop {
        let mut options = QueryOptions::default().with_limit(1000);
        if let Some(c) = cursor.clone() {
            options.set_after(c);
        }
        let mut request = ListTransactionsRequest::default()
            .with_read_mask(read_mask.clone())
            .with_start_checkpoint(start_checkpoint)
            .with_filter(filter.clone())
            .with_options(options);
        if end_checkpoint_exclusive > start_checkpoint {
            request = request.with_end_checkpoint(end_checkpoint_exclusive);
        }

        let mut response = client
            .list_transactions(request)
            .await
            .map_err(ClientError::RpcError)?
            .into_inner();

        let mut end_reason: Option<QueryEndReason> = None;
        let mut last_cursor: Option<Vec<u8>> = None;

        while let Some(frame) = response.next().await {
            let frame = frame.map_err(ClientError::RpcError)?;
            match frame.response {
                Some(list_transactions_response::Response::Item(item)) => {
                    if let Some(c) = item.watermark.as_ref().and_then(|w| w.cursor.as_ref()) {
                        last_cursor = Some(c.to_vec());
                    }
                    let checkpoint = item
                        .transaction
                        .as_ref()
                        .and_then(|tx| tx.checkpoint)
                        .ok_or_else(|| {
                            ClientError::InternalError(
                                "settlement transaction missing checkpoint".to_string(),
                            )
                        })?;
                    let tx_offset = item
                        .transaction
                        .as_ref()
                        .and_then(|tx| tx.transaction_index)
                        .ok_or_else(|| {
                            ClientError::InternalError(
                                "settlement transaction missing transaction_index".to_string(),
                            )
                        })?;
                    all.push((checkpoint, tx_offset));
                }
                Some(list_transactions_response::Response::Watermark(w)) => {
                    if let Some(c) = w.cursor.as_ref() {
                        last_cursor = Some(c.to_vec());
                    }
                }
                Some(list_transactions_response::Response::End(end)) => {
                    end_reason = Some(end.reason());
                }
                Some(_) | None => {}
            }
        }

        if matches!(
            end_reason,
            Some(QueryEndReason::ItemLimit) | Some(QueryEndReason::ScanLimit)
        ) {
            // More work remains for this scan; resume from the watermark
            // cursor on the next request.
            cursor = last_cursor;
            continue;
        }
        break;
    }

    Ok(all)
}

fn build_affected_object_filter(object_id: ObjectID) -> TransactionFilter {
    let object_filter = AffectedObjectFilter::default().with_object_id(object_id.to_string());
    let predicate = TransactionPredicate::default().with_affected_object(object_filter);
    let literal = TransactionLiteral::default().with_include(predicate);
    let term = TransactionTerm::default().with_literals(vec![literal]);
    TransactionFilter::default().with_terms(vec![term])
}

fn build_event_stream_head_filter(stream_id: SuiAddress) -> EventFilter {
    let head_filter = EventStreamHeadFilter::default().with_stream_id(stream_id.to_string());
    let predicate = EventPredicate::default().with_event_stream_head(head_filter);
    let literal = EventLiteral::default().with_include(predicate);
    let term = EventTerm::default().with_literals(vec![literal]);
    EventFilter::default().with_terms(vec![term])
}

pub(crate) async fn create_event_stream_with_head(
    client: Arc<AuthenticatedEventsClient>,
    stream_id: SuiAddress,
    stream_object_id: ObjectID,
    start_checkpoint: u64,
    initial_head: Option<EventStreamHead>,
    config: ClientConfig,
) -> Result<impl Stream<Item = Result<AuthenticatedEvent, ClientError>>, ClientError> {
    let (tx, rx) = mpsc::channel(1000);

    let poll_interval = config.poll_interval;
    let mut state = EventStreamState::new(
        client,
        stream_id,
        stream_object_id,
        start_checkpoint,
        initial_head,
        config,
    );

    // TODO: Add exponential backoff for transient errors
    tokio::spawn(async move {
        loop {
            match state.fetch_and_verify_next_batch().await {
                Ok(events) => {
                    if events.is_empty() {
                        tokio::task::yield_now().await;
                        tokio::time::sleep(poll_interval).await;
                        continue;
                    }

                    for event in events {
                        if tx.send(Ok(event)).await.is_err() {
                            return;
                        }
                    }

                    tokio::time::sleep(poll_interval).await;
                }
                Err(e) => {
                    if e.is_terminal() {
                        tracing::error!(
                            "Terminal error in event stream, no more events will be produced: {:?}",
                            e
                        );
                        let _ = tx.send(Err(e)).await;
                        return;
                    }

                    tracing::warn!(
                        "Retryable event stream error: {:?}. Retrying after {:?}",
                        e,
                        poll_interval
                    );

                    tokio::time::sleep(poll_interval).await;
                }
            }
        }
    });

    Ok(tokio_stream::wrappers::ReceiverStream::new(rx))
}

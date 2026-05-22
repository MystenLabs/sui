// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::mmr::apply_stream_updates;
use super::{AuthenticatedEvent, AuthenticatedEventsClient, ClientConfig, ClientError};
use futures::StreamExt;
use futures::stream::Stream;
use mysten_common::debug_fatal;
use std::sync::Arc;
use sui_rpc::field::{FieldMask, FieldMaskUtil};
use sui_rpc::proto::sui::rpc::v2alpha::{
    EventFilter, EventLiteral, EventPredicate, EventStreamHeadFilter, EventTerm, ListEventsRequest,
    QueryEndReason, QueryOptions, list_events_response,
};
use sui_types::accumulator_root::{EventCommitment, EventStreamHead};
use sui_types::base_types::{ObjectID, SuiAddress};
use tokio::sync::mpsc;

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

        let mut options = QueryOptions::default().with_limit_items(self.config.page_size);
        if let Some(cursor) = self.next_cursor.as_ref() {
            options.set_after(cursor.clone());
        }
        // Request every field the in-tree `AuthenticatedEvent` converter
        // needs. The server's default event read mask drops `contents`, so
        // we ask for the full event explicitly.
        let read_mask =
            FieldMask::from_paths(["package_id", "module", "sender", "event_type", "contents"]);
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
                    end_reason = QueryEndReason::try_from(end.reason).ok();
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

        let accumulated_events =
            Self::group_events_by_checkpoint(events, self.last_verified_checkpoint);

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

    /// Group events by their containing checkpoint, in scan order. The new
    /// v2alpha API doesn't expose the per-settlement `accumulator_version`,
    /// so we match the on-chain MMR fold by treating every checkpoint as a
    /// single batch — the framework's `apply_stream_updates` folds one
    /// per-checkpoint Merkle root into the MMR at a time.
    fn group_events_by_checkpoint(
        events: &[AuthenticatedEvent],
        last_verified_checkpoint: u64,
    ) -> Vec<Vec<EventCommitment>> {
        let mut accumulated_events: Vec<Vec<EventCommitment>> = Vec::new();
        let mut current_checkpoint: Option<u64> = None;
        let mut current_batch: Vec<EventCommitment> = Vec::new();

        for event in events {
            if event.checkpoint <= last_verified_checkpoint {
                debug_fatal!(
                    "Received event from checkpoint {} which is <= last_verified_checkpoint {}",
                    event.checkpoint,
                    last_verified_checkpoint
                );
                continue;
            }

            let digest = event.event.digest();
            let commitment = EventCommitment::new(
                event.checkpoint,
                event.transaction_offset,
                event.event_index as u64,
                digest,
            );

            match current_checkpoint {
                None => {
                    current_checkpoint = Some(event.checkpoint);
                    current_batch.push(commitment);
                }
                Some(checkpoint) if checkpoint == event.checkpoint => {
                    current_batch.push(commitment);
                }
                Some(_) => {
                    accumulated_events.push(std::mem::take(&mut current_batch));
                    current_batch.push(commitment);
                    current_checkpoint = Some(event.checkpoint);
                }
            }
        }

        if !current_batch.is_empty() {
            accumulated_events.push(current_batch);
        }

        accumulated_events
    }
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

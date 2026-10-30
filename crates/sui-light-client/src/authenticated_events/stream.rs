// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::mmr::apply_stream_updates;
use super::{AuthenticatedEvent, AuthenticatedEventsClient, ClientConfig, ClientError};
use futures::StreamExt;
use futures::stream::{BoxStream, Stream};
use mysten_common::debug_fatal;
use std::sync::Arc;
use sui_rpc::client::{
    Delivery, EventStreamFrame, EventStreamRequest, EventStreamStart, LedgerStreamConfig,
    ListConfig,
};
use sui_rpc::field::{FieldMask, FieldMaskUtil};
use sui_rpc::proto::sui::rpc::v2::{
    AffectedObjectFilter, EventFilter, EventLiteral, EventStreamHeadFilter, EventTerm,
    ListTransactionsRequest, QueryOptions, TransactionFilter, TransactionLiteral, TransactionTerm,
};
use sui_types::accumulator_root::{EventCommitment, EventStreamHead};
use sui_types::base_types::{ObjectID, SuiAddress};
use tokio::sync::mpsc;

/// Maximum settlement transactions per `ListTransactions` page.
const SETTLEMENT_PAGE_LIMIT: u32 = 1000;

/// Events delivered by the event stream that have not been verified yet, together with
/// the server's claim of how far delivery is complete.
///
/// A checkpoint is complete only once the server claims it: a page can end in the
/// middle of a checkpoint, and the rest of that checkpoint may arrive pages later (for
/// example after a lagging replica catches up). Folding a partial checkpoint into the
/// MMR would never match the on-chain head, so only events at complete checkpoints are
/// verified and released.
#[derive(Debug, Default)]
struct PendingEvents {
    /// Buffered events in ledger order.
    events: Vec<AuthenticatedEvent>,
    /// Highest `covered_checkpoint` the stream has reported: every matching event at a
    /// checkpoint `<=` it has been delivered.
    covered_checkpoint: Option<u64>,
}

impl PendingEvents {
    /// Records one stream frame and returns whether a verification round is due.
    ///
    /// A round is due when a progress-only frame shows that every buffered event is at a
    /// complete checkpoint (the stream has caught up), or when the events at complete
    /// checkpoints reach `batch_limit` (bounding the buffer while catching up).
    fn record_frame(
        &mut self,
        event: Option<AuthenticatedEvent>,
        covered_checkpoint: Option<u64>,
        batch_limit: usize,
    ) -> bool {
        if let Some(covered) = covered_checkpoint {
            self.covered_checkpoint = Some(
                self.covered_checkpoint
                    .map_or(covered, |previous| previous.max(covered)),
            );
        }
        match event {
            Some(event) => {
                self.events.push(event);
                self.complete_events().len() >= batch_limit
            }
            None => !self.events.is_empty() && self.complete_events().len() == self.events.len(),
        }
    }

    /// Buffered events at checkpoints the server has claimed complete, in ledger order.
    fn complete_events(&self) -> &[AuthenticatedEvent] {
        let Some(covered) = self.covered_checkpoint else {
            return &[];
        };
        let end = self
            .events
            .partition_point(|event| event.checkpoint <= covered);
        &self.events[..end]
    }

    /// Removes and returns the buffered events at checkpoints `<= checkpoint`.
    fn release_through(&mut self, checkpoint: u64) -> Vec<AuthenticatedEvent> {
        let end = self
            .events
            .partition_point(|event| event.checkpoint <= checkpoint);
        self.events.drain(..end).collect()
    }
}

/// Settle_events transactions listed for a checkpoint range.
struct SettlementListing {
    /// `(checkpoint, transaction_offset)` of every settlement at a checkpoint `<=
    /// covered_checkpoint`, ascending.
    settlements: Vec<(u64, u64)>,
    /// Highest checkpoint the server claimed fully listed, or `None` when the listing
    /// covered no checkpoint of the range.
    covered_checkpoint: Option<u64>,
}

struct EventStreamState {
    /// Ledger-stream facade over `ListEvents`, following the tip by polling. It retries
    /// transient RPC errors itself, so an error it yields is final.
    frames: BoxStream<'static, Result<EventStreamFrame, tonic::Status>>,
    pending: PendingEvents,
    /// Whether the next step verifies the complete pending events. Stays set while a
    /// verification round fails with a retriable error.
    verification_due: bool,
    /// Number of events at complete checkpoints that triggers a verification round
    /// before the stream has caught up.
    verification_batch_limit: usize,
    verifier: StreamVerifier,
}

/// The verified position of the event stream.
struct StreamVerifier {
    client: Arc<AuthenticatedEventsClient>,
    stream_object_id: ObjectID,
    last_verified_checkpoint: u64,
    verified_stream_head: Option<EventStreamHead>,
}

impl EventStreamState {
    fn new(
        client: Arc<AuthenticatedEventsClient>,
        stream_id: SuiAddress,
        stream_object_id: ObjectID,
        start_checkpoint: u64,
        initial_stream_head: Option<EventStreamHead>,
        config: &ClientConfig,
    ) -> Self {
        // Request every field the `AuthenticatedEvent` conversion needs: the event body
        // plus its ledger-position fields (`checkpoint`, `transaction_index`,
        // `event_index`), which the list endpoint only populates when the read mask asks
        // for them.
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
        let request = EventStreamRequest::new()
            .with_read_mask(read_mask)
            .with_filter(build_event_stream_head_filter(stream_id))
            .with_start(EventStreamStart::Checkpoint(start_checkpoint))
            .with_delivery(Delivery::Poll);
        let mut stream_config = LedgerStreamConfig::default();
        stream_config.ledger_tip_poll_interval = config.poll_interval;
        stream_config.list_page_limit = Some(config.page_size);
        let frames = client
            .rpc_client()
            .stream_events_with_config(request, stream_config)
            .boxed();

        Self {
            frames,
            pending: PendingEvents::default(),
            verification_due: false,
            verification_batch_limit: config.page_size as usize,
            verifier: StreamVerifier {
                client,
                stream_object_id,
                last_verified_checkpoint: start_checkpoint.saturating_sub(1),
                verified_stream_head: initial_stream_head,
            },
        }
    }

    /// Reads frames until a verification round releases events, and returns them.
    async fn next_verified_events(&mut self) -> Result<Vec<AuthenticatedEvent>, ClientError> {
        loop {
            if self.verification_due {
                let released = self.verify_complete_events().await?;
                self.verification_due = false;
                if !released.is_empty() {
                    return Ok(released);
                }
            }

            let frame = match self.frames.next().await {
                Some(frame) => frame.map_err(ClientError::RpcError)?,
                None => {
                    return Err(ClientError::InternalError(
                        "event stream ended after its final error".to_string(),
                    ));
                }
            };
            let event = frame.event.map(AuthenticatedEvent::try_from).transpose()?;
            self.verification_due = self.pending.record_frame(
                event,
                frame.covered_checkpoint,
                self.verification_batch_limit,
            );
        }
    }

    /// Verifies the longest prefix of the complete pending events whose settlements are
    /// listed, and releases it. Events past that prefix stay buffered for a later round.
    async fn verify_complete_events(&mut self) -> Result<Vec<AuthenticatedEvent>, ClientError> {
        let complete = self.pending.complete_events();
        let (Some(first), Some(last)) = (complete.first(), complete.last()) else {
            return Ok(Vec::new());
        };

        let listing = list_settlements(
            self.verifier.client.rpc_client(),
            self.verifier.stream_object_id,
            first.checkpoint,
            last.checkpoint.saturating_add(1),
        )
        .await?;
        let verifiable = settled_prefix(complete, listing.covered_checkpoint);
        // The stream head is proven at the last verifiable event's checkpoint, which
        // holds a settlement because the stream emitted events there.
        let Some(up_to_checkpoint) = verifiable.last().map(|event| event.checkpoint) else {
            return Ok(Vec::new());
        };

        self.verifier
            .verify_events(up_to_checkpoint, verifiable, &listing.settlements)
            .await?;

        Ok(self.pending.release_through(up_to_checkpoint))
    }
}

/// The prefix of `complete` (complete pending events, in ledger order) whose settlements
/// the listing claimed through `settled_through`; empty when it claimed none.
fn settled_prefix(
    complete: &[AuthenticatedEvent],
    settled_through: Option<u64>,
) -> &[AuthenticatedEvent] {
    let Some(settled_through) = settled_through else {
        return &[];
    };
    let end = complete.partition_point(|event| event.checkpoint <= settled_through);
    &complete[..end]
}

impl StreamVerifier {
    /// Folds `events` (every event of the stream in `(last_verified_checkpoint,
    /// up_to_checkpoint]`) into the verified head, checks the result against the stream
    /// head proven at `up_to_checkpoint`, and advances the verified position to it.
    async fn verify_events(
        &mut self,
        up_to_checkpoint: u64,
        events: &[AuthenticatedEvent],
        settlements: &[(u64, u64)],
    ) -> Result<(), ClientError> {
        tracing::info!(
            "Verifying {} events from checkpoint {} to {}",
            events.len(),
            self.last_verified_checkpoint,
            up_to_checkpoint
        );

        let default_head = EventStreamHead::default();
        let old_head = self.verified_stream_head.as_ref().unwrap_or(&default_head);

        let accumulated_events =
            Self::bucket_events_by_settlement(events, settlements, self.last_verified_checkpoint)?;

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
/// The listing ends early, without an error, when it reaches the tip of a
/// lagging replica. Only settlements at checkpoints the server claimed fully
/// listed are returned, sorted ascending — the same order the events stream
/// uses, which lets downstream bucketing walk both with a single cursor.
async fn list_settlements(
    client: &sui_rpc::Client,
    stream_object_id: ObjectID,
    start_checkpoint: u64,
    end_checkpoint_exclusive: u64,
) -> Result<SettlementListing, ClientError> {
    // We need `checkpoint` and the `transaction_index` position field from each
    // settlement transaction.
    let request = ListTransactionsRequest::default()
        .with_read_mask(FieldMask::from_paths(["checkpoint", "transaction_index"]))
        .with_start_checkpoint(start_checkpoint)
        .with_end_checkpoint(end_checkpoint_exclusive)
        .with_filter(build_affected_object_filter(stream_object_id))
        .with_options(QueryOptions::default().with_limit(SETTLEMENT_PAGE_LIMIT));

    let mut settlements: Vec<(u64, u64)> = Vec::new();
    let mut covered_checkpoint: Option<u64> = None;
    let mut responses =
        std::pin::pin!(client.list_transactions_with_config(request, ListConfig::default()));
    while let Some(response) = responses.next().await {
        let response = response.map_err(ClientError::RpcError)?;
        if let Some(covered) = response
            .watermark
            .as_ref()
            .and_then(|watermark| watermark.checkpoint)
        {
            covered_checkpoint =
                Some(covered_checkpoint.map_or(covered, |previous| previous.max(covered)));
        }
        if let Some(transaction) = response.transaction {
            let checkpoint = transaction.checkpoint.ok_or_else(|| {
                ClientError::InternalError("settlement transaction missing checkpoint".to_string())
            })?;
            let tx_offset = transaction.transaction_index.ok_or_else(|| {
                ClientError::InternalError(
                    "settlement transaction missing transaction_index".to_string(),
                )
            })?;
            settlements.push((checkpoint, tx_offset));
        }
    }

    settlements
        .retain(|(checkpoint, _)| covered_checkpoint.is_some_and(|covered| *checkpoint <= covered));
    Ok(SettlementListing {
        settlements,
        covered_checkpoint,
    })
}

fn build_affected_object_filter(object_id: ObjectID) -> TransactionFilter {
    let object_filter = AffectedObjectFilter::default().with_object_id(object_id.to_string());
    let literal = TransactionLiteral::default().with_affected_object(object_filter);
    let term = TransactionTerm::default().with_literals(vec![literal]);
    TransactionFilter::default().with_terms(vec![term])
}

fn build_event_stream_head_filter(stream_id: SuiAddress) -> EventFilter {
    let head_filter = EventStreamHeadFilter::default().with_stream_id(stream_id.to_string());
    let literal = EventLiteral::default().with_event_stream_head(head_filter);
    let term = EventTerm::default().with_literals(vec![literal]);
    EventFilter::default().with_terms(vec![term])
}

/// Streams the verified events of `stream_id` from `start_checkpoint` (inclusive).
/// `initial_head` is the verified stream head at `start_checkpoint - 1`, or `None` when
/// the stream starts from genesis.
pub(crate) fn create_event_stream_with_head(
    client: Arc<AuthenticatedEventsClient>,
    stream_id: SuiAddress,
    stream_object_id: ObjectID,
    start_checkpoint: u64,
    initial_head: Option<EventStreamHead>,
    config: ClientConfig,
) -> impl Stream<Item = Result<AuthenticatedEvent, ClientError>> {
    let (tx, rx) = mpsc::channel(1000);

    let retry_interval = config.poll_interval;
    let mut state = EventStreamState::new(
        client,
        stream_id,
        stream_object_id,
        start_checkpoint,
        initial_head,
        &config,
    );

    tokio::spawn(async move {
        loop {
            match state.next_verified_events().await {
                Ok(events) => {
                    for event in events {
                        if tx.send(Ok(event)).await.is_err() {
                            return;
                        }
                    }
                }
                Err(e) if e.is_terminal() => {
                    tracing::error!(
                        "Terminal error in event stream, no more events will be produced: {:?}",
                        e
                    );
                    let _ = tx.send(Err(e)).await;
                    return;
                }
                Err(e) => {
                    tracing::warn!(
                        "Retryable event stream error: {:?}. Retrying after {:?}",
                        e,
                        retry_interval
                    );
                    tokio::time::sleep(retry_interval).await;
                }
            }
        }
    });

    tokio_stream::wrappers::ReceiverStream::new(rx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sui_types::event::Event;

    fn event_at(checkpoint: u64, transaction_offset: u64, event_index: u32) -> AuthenticatedEvent {
        AuthenticatedEvent {
            event: Event::random_for_testing(),
            checkpoint,
            transaction_offset,
            event_index,
        }
    }

    fn checkpoints(events: &[AuthenticatedEvent]) -> Vec<u64> {
        events.iter().map(|event| event.checkpoint).collect()
    }

    /// A page that splits checkpoint 10, followed by an empty page from a lagging replica
    /// (a progress-only frame whose claim stays below 10), must leave checkpoint 10
    /// unverified and unreleased until the server claims it complete.
    #[test]
    fn checkpoint_is_incomplete_until_the_server_claims_it() {
        const BATCH_LIMIT: usize = 2;
        let mut pending = PendingEvents::default();

        assert!(!pending.record_frame(Some(event_at(9, 0, 0)), Some(8), BATCH_LIMIT));
        // The first events of checkpoint 10 arrive while the claim only reaches 9.
        assert!(!pending.record_frame(Some(event_at(10, 1, 0)), Some(9), BATCH_LIMIT));
        assert!(!pending.record_frame(Some(event_at(10, 1, 1)), Some(9), BATCH_LIMIT));
        // A lagging replica answers with an empty page: progress-only frames whose
        // claim is inherited or absent.
        assert!(!pending.record_frame(None, Some(9), BATCH_LIMIT));
        assert!(!pending.record_frame(None, None, BATCH_LIMIT));
        assert_eq!(checkpoints(pending.complete_events()), vec![9]);

        // The rest of checkpoint 10 arrives, still under a claim of 9.
        assert!(!pending.record_frame(Some(event_at(10, 2, 0)), Some(9), BATCH_LIMIT));
        assert_eq!(checkpoints(pending.complete_events()), vec![9]);

        // Once the claim reaches 10, every buffered event is complete and the stream has
        // caught up.
        assert!(pending.record_frame(None, Some(10), BATCH_LIMIT));
        assert_eq!(checkpoints(pending.complete_events()), vec![9, 10, 10, 10]);
        assert_eq!(
            checkpoints(&pending.release_through(10)),
            vec![9, 10, 10, 10]
        );
        assert!(pending.events.is_empty());
    }

    /// Settlements listed only through an earlier checkpoint verify only that prefix; the
    /// later complete events stay buffered.
    #[test]
    fn only_events_with_listed_settlements_are_verifiable() {
        let mut pending = PendingEvents::default();
        pending.record_frame(Some(event_at(9, 0, 0)), None, usize::MAX);
        pending.record_frame(Some(event_at(10, 1, 0)), None, usize::MAX);
        pending.record_frame(Some(event_at(10, 1, 1)), Some(10), usize::MAX);
        let complete = pending.complete_events();

        assert!(settled_prefix(complete, None).is_empty());
        assert!(settled_prefix(complete, Some(8)).is_empty());
        assert_eq!(checkpoints(settled_prefix(complete, Some(9))), vec![9]);
        assert_eq!(
            checkpoints(settled_prefix(complete, Some(10))),
            vec![9, 10, 10]
        );

        assert_eq!(checkpoints(&pending.release_through(9)), vec![9]);
        assert_eq!(checkpoints(&pending.events), vec![10, 10]);
    }
}

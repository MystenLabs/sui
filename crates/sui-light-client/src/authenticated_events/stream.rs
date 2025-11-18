// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{AuthenticatedEvent, AuthenticatedEventsClient, ClientConfig, ClientError};
use crate::mmr::apply_stream_updates;
use futures::stream::Stream;
use mysten_common::debug_fatal;
use std::sync::Arc;
use sui_rpc_api::grpc::alpha::event_service_proto::ListAuthenticatedEventsRequest;
use sui_types::accumulator_root::{EventCommitment, EventStreamHead};
use sui_types::base_types::{ObjectID, SuiAddress};
use tokio::sync::mpsc;

struct EventStreamState {
    client: Arc<AuthenticatedEventsClient>,
    stream_id: SuiAddress,
    stream_object_id: ObjectID,
    current_checkpoint: u64,
    config: ClientConfig,
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
        config: ClientConfig,
    ) -> Self {
        Self {
            client,
            stream_id,
            stream_object_id,
            current_checkpoint: start_checkpoint,
            config,
            last_verified_checkpoint: start_checkpoint.saturating_sub(1),
            verified_stream_head: initial_stream_head,
        }
    }

    async fn fetch_and_verify_next_batch(
        &mut self,
    ) -> Result<Vec<AuthenticatedEvent>, ClientError> {
        let mut parsed_events: Vec<AuthenticatedEvent> = Vec::new();
        let mut page_token: Option<Vec<u8>> = None;
        let start_checkpoint = self.current_checkpoint;
        let mut iteration_count = 0;

        loop {
            iteration_count += 1;

            let response = {
                let mut event_service = self.client.event_service();
                let mut request = ListAuthenticatedEventsRequest::default();
                request.stream_id = Some(self.stream_id.to_string());
                request.start_checkpoint = Some(start_checkpoint);
                request.page_size = Some(self.config.page_size);
                request.page_token = page_token.clone();

                event_service
                    .list_authenticated_events(request)
                    .await
                    .map_err(ClientError::RpcError)?
                    .into_inner()
            };

            let events = response
                .events
                .into_iter()
                .map(|event| event.try_into())
                .collect::<Result<Vec<_>, _>>()?;
            parsed_events.extend(events);

            let has_more_pages = response
                .next_page_token
                .as_ref()
                .filter(|t| !t.is_empty())
                .is_some();

            if !has_more_pages {
                break;
            }

            if iteration_count >= self.config.max_pagination_iterations {
                let last_complete_checkpoint = Self::truncate_to_last_complete_checkpoint(
                    &mut parsed_events,
                )
                .ok_or_else(|| {
                    ClientError::InternalError(
                        "Hit pagination limit but no complete checkpoints found".to_string(),
                    )
                })?;

                tracing::warn!(
                    "Hit pagination iteration limit of {}. Truncated to {} events, last complete checkpoint is {}",
                    self.config.max_pagination_iterations,
                    parsed_events.len(),
                    last_complete_checkpoint
                );
                break;
            }

            page_token = response.next_page_token;
        }

        if parsed_events.is_empty() {
            return Ok(Vec::new());
        }

        if let Some(last_event) = parsed_events.last() {
            let last_checkpoint = last_event.checkpoint;
            if last_checkpoint > self.last_verified_checkpoint
                && let Err(e) = self.verify_events(last_checkpoint, &parsed_events).await
            {
                debug_fatal!(
                    "Failed to verify events up to checkpoint {}: {:?}",
                    last_checkpoint,
                    e
                );
                return Err(e);
            }
            self.current_checkpoint = last_checkpoint + 1;
        }

        Ok(parsed_events)
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

        if new_stream_head.checkpoint_seq < old_head.checkpoint_seq {
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

        self.verified_stream_head = Some(new_stream_head);
        self.last_verified_checkpoint = up_to_checkpoint;

        Ok(())
    }

    fn group_events_by_checkpoint(
        events: &[AuthenticatedEvent],
        last_verified_checkpoint: u64,
    ) -> Vec<Vec<EventCommitment>> {
        let mut accumulated_events: Vec<Vec<EventCommitment>> = Vec::new();
        let mut current_accumulator_version: Option<u64> = None;
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
                event.transaction_idx as u64,
                event.event_idx as u64,
                digest,
            );

            match current_accumulator_version {
                None => {
                    current_accumulator_version = Some(event.accumulator_version);
                    current_batch.push(commitment);
                }
                Some(version) if version == event.accumulator_version => {
                    current_batch.push(commitment);
                }
                Some(_) => {
                    accumulated_events.push(current_batch);
                    current_batch = vec![commitment];
                    current_accumulator_version = Some(event.accumulator_version);
                }
            }
        }

        if !current_batch.is_empty() {
            accumulated_events.push(current_batch);
        }

        accumulated_events
    }

    fn truncate_to_last_complete_checkpoint(events: &mut Vec<AuthenticatedEvent>) -> Option<u64> {
        if events.is_empty() {
            return None;
        }

        let last_checkpoint = events.last().unwrap().checkpoint;

        let truncate_at = events
            .iter()
            .rposition(|e| e.checkpoint != last_checkpoint)?;

        events.truncate(truncate_at + 1);

        events.last().map(|e| e.checkpoint)
    }
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

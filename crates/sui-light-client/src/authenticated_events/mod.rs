// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod converters;
mod epoch_cache;
pub mod mmr;
mod stream;

use crate::proof::base::{Proof, ProofContents, ProofTarget, ProofVerifier};
use crate::proof::committee::extract_new_committee_info;
use crate::proof::error::ProofError;
use crate::proof::ocs::{OCSProof, OCSTarget};
use epoch_cache::EpochCache;
use futures::stream::Stream;
use move_core_types::identifier::Identifier;
use std::sync::Arc;
use std::time::Duration;
use sui_rpc::field::{FieldMask, FieldMaskUtil};
use sui_rpc_api::grpc::alpha::event_service_proto::event_service_client::EventServiceClient;
use sui_rpc_api::grpc::alpha::proof_service_proto::proof_service_client::ProofServiceClient;
use sui_rpc_api::proto::sui::rpc::v2::ledger_service_client::LedgerServiceClient;
use sui_rpc_api::proto::sui::rpc::v2::{GetCheckpointRequest, GetEpochRequest};
use sui_types::accumulator_root::{EventStreamHead, derive_event_stream_head_object_id};
use sui_types::base_types::SuiAddress;
use sui_types::committee::Committee;
use sui_types::event::Event;
use thiserror::Error;
use tonic::transport::Channel;

/// A cryptographically verified event.
///
/// Each `AuthenticatedEvent` can be verified using the EventStreamHead's MMR
/// (Merkle Mountain Range) commitment, ensuring its authenticity and inclusion
/// at the specified checkpoint.
#[derive(Debug, Clone)]
pub struct AuthenticatedEvent {
    /// The underlying Sui event data.
    pub event: Event,
    /// The checkpoint sequence number where this event was included.
    pub checkpoint: u64,
    /// The accumulator version when this event was committed.
    pub accumulator_version: u64,
    /// The transaction index within the checkpoint.
    pub transaction_idx: u32,
    /// The event index within the transaction.
    pub event_idx: u32,
}

impl TryFrom<sui_rpc_api::grpc::alpha::event_service_proto::AuthenticatedEvent>
    for AuthenticatedEvent
{
    type Error = ClientError;

    fn try_from(
        event: sui_rpc_api::grpc::alpha::event_service_proto::AuthenticatedEvent,
    ) -> Result<Self, Self::Error> {
        let proto_event = event
            .event
            .ok_or_else(|| ClientError::InternalError("Missing event data".to_string()))?;

        let contents = proto_event
            .contents
            .ok_or_else(|| ClientError::InternalError("Missing event contents".to_string()))?;

        let event_bytes = contents
            .value
            .ok_or_else(|| ClientError::InternalError("Missing event value".to_string()))?;

        let package_id = proto_event
            .package_id
            .as_ref()
            .ok_or_else(|| ClientError::InternalError("Missing package_id".to_string()))?;
        let package_id = sui_types::base_types::ObjectID::from_hex_literal(package_id)
            .map_err(|e| ClientError::InternalError(format!("Invalid package_id: {}", e)))?;

        let module = proto_event
            .module
            .as_ref()
            .ok_or_else(|| ClientError::InternalError("Missing module".to_string()))?;
        let module = Identifier::new(module.as_str())
            .map_err(|e| ClientError::InternalError(format!("Invalid module: {}", e)))?;

        let sender = proto_event
            .sender
            .as_ref()
            .ok_or_else(|| ClientError::InternalError("Missing sender".to_string()))?;
        let sender = sender
            .parse()
            .map_err(|e| ClientError::InternalError(format!("Invalid sender: {}", e)))?;

        let event_type = proto_event
            .event_type
            .as_ref()
            .ok_or_else(|| ClientError::InternalError("Missing event_type".to_string()))?;
        let type_tag: move_core_types::language_storage::StructTag = event_type
            .parse()
            .map_err(|e| ClientError::InternalError(format!("Invalid event_type: {}", e)))?;

        let event_data = Event {
            package_id,
            transaction_module: module,
            sender,
            type_: type_tag,
            contents: event_bytes.to_vec(),
        };

        let checkpoint = event
            .checkpoint
            .ok_or_else(|| ClientError::InternalError("Missing checkpoint".to_string()))?;
        let transaction_idx = event
            .transaction_idx
            .ok_or_else(|| ClientError::InternalError("Missing transaction_idx".to_string()))?;
        let event_idx = event
            .event_idx
            .ok_or_else(|| ClientError::InternalError("Missing event_idx".to_string()))?;
        let accumulator_version = event
            .accumulator_version
            .ok_or_else(|| ClientError::InternalError("Missing accumulator_version".to_string()))?;

        Ok(AuthenticatedEvent {
            event: event_data,
            checkpoint,
            accumulator_version,
            transaction_idx,
            event_idx,
        })
    }
}

/// Prevent ratcheting to an arbitrarily large epoch in case of malformed node response.
const MAX_RATCHET_GAP: u64 = 10_000;

/// Maximum number of epoch fetches to run concurrently during trust ratcheting.
const RATCHET_FETCH_CONCURRENCY: usize = 20;

struct EpochCheckpointData {
    epoch: u64,
    end_checkpoint_seq: u64,
    summary: sui_types::messages_checkpoint::CertifiedCheckpointSummary,
}

/// Configuration for the authenticated events client.
///
/// Controls streaming behavior (page size, polling, pagination) and RPC communication (timeouts).
#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub page_size: u32,
    pub poll_interval: Duration,
    pub max_pagination_iterations: usize,
    pub rpc_timeout: Duration,
}

impl ClientConfig {
    pub fn new(
        page_size: u32,
        poll_interval: Duration,
        max_pagination_iterations: usize,
        rpc_timeout: Duration,
    ) -> Result<Self, String> {
        if page_size == 0 {
            return Err("page_size must be greater than 0".to_string());
        }
        if page_size > 1000 {
            return Err("page_size must not exceed 1000 (server limit)".to_string());
        }
        if poll_interval.is_zero() {
            return Err("poll_interval must be greater than 0".to_string());
        }
        if max_pagination_iterations == 0 {
            return Err("max_pagination_iterations must be greater than 0".to_string());
        }
        if rpc_timeout.is_zero() {
            return Err("rpc_timeout must be greater than 0".to_string());
        }

        Ok(Self {
            page_size,
            poll_interval,
            max_pagination_iterations,
            rpc_timeout,
        })
    }
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            page_size: 1000,
            poll_interval: Duration::from_secs(1),
            max_pagination_iterations: 100,
            rpc_timeout: Duration::from_secs(30),
        }
    }
}

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("Verification failed: {0}")]
    VerificationError(String),

    #[error("Internal error: {0}")]
    InternalError(String),

    #[error("RPC error: {0}")]
    RpcError(#[from] tonic::Status),

    #[error("Transport error: {0}")]
    TransportError(#[from] tonic::transport::Error),
}

impl From<bcs::Error> for ClientError {
    fn from(e: bcs::Error) -> Self {
        ClientError::InternalError(format!("BCS deserialization failed: {}", e))
    }
}

impl From<ProofError> for ClientError {
    fn from(e: ProofError) -> Self {
        ClientError::VerificationError(e.to_string())
    }
}

impl ClientError {
    pub(crate) fn is_terminal(&self) -> bool {
        match self {
            ClientError::RpcError(status) => !Self::is_retriable_grpc_code(status.code()),
            ClientError::TransportError(_) => false,
            _ => true,
        }
    }

    fn is_retriable_grpc_code(code: tonic::Code) -> bool {
        matches!(
            code,
            tonic::Code::Unavailable
                | tonic::Code::DeadlineExceeded
                | tonic::Code::ResourceExhausted
                | tonic::Code::Aborted
        )
    }
}

pub struct AuthenticatedEventsClient {
    event_service: EventServiceClient<tonic::transport::Channel>,
    proof_service: ProofServiceClient<tonic::transport::Channel>,
    ledger_service: LedgerServiceClient<tonic::transport::Channel>,
    epoch_cache: Arc<tokio::sync::Mutex<EpochCache>>,
    config: ClientConfig,
}

impl AuthenticatedEventsClient {
    pub async fn new(rpc_url: &str, genesis_committee: Committee) -> Result<Self, ClientError> {
        Self::new_with_config(rpc_url, genesis_committee, ClientConfig::default()).await
    }

    pub async fn new_with_config(
        rpc_url: &str,
        genesis_committee: Committee,
        config: ClientConfig,
    ) -> Result<Self, ClientError> {
        let mut endpoint = Channel::from_shared(rpc_url.to_string())
            .map_err(|e| ClientError::InternalError(format!("Invalid RPC URL: {}", e)))?
            .connect_timeout(Duration::from_secs(5))
            .timeout(config.rpc_timeout);

        if rpc_url.starts_with("https") {
            endpoint = endpoint
                .tls_config(tonic::transport::ClientTlsConfig::new().with_webpki_roots())
                .map_err(|e| ClientError::InternalError(format!("TLS config error: {}", e)))?;
        }

        const MAX_RETRIES: u32 = 10;
        let mut last_err = None;
        for _ in 0..MAX_RETRIES {
            match endpoint.connect().await {
                Ok(ch) => {
                    let event_service = EventServiceClient::new(ch.clone());
                    let proof_service = ProofServiceClient::new(ch.clone());
                    let ledger_service = LedgerServiceClient::new(ch);
                    let epoch_cache = EpochCache::new(genesis_committee);

                    return Ok(Self {
                        event_service,
                        proof_service,
                        ledger_service,
                        epoch_cache: Arc::new(tokio::sync::Mutex::new(epoch_cache)),
                        config,
                    });
                }
                Err(e) => {
                    last_err = Some(e);
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }
        Err(last_err.unwrap().into())
    }

    fn extract_stream_head_from_object(
        object: &sui_types::object::Object,
    ) -> Result<EventStreamHead, ClientError> {
        match &object.data {
            sui_types::object::Data::Move(move_obj) => {
                let field: sui_types::dynamic_field::Field<
                    sui_types::accumulator_root::AccumulatorKey,
                    EventStreamHead,
                > = move_obj.to_rust().ok_or_else(|| {
                    ClientError::InternalError("Failed to deserialize EventStreamHead".to_string())
                })?;
                Ok(field.value)
            }
            sui_types::object::Data::Package(_) => Err(ClientError::InternalError(
                "Expected Move object, got Package".to_string(),
            )),
        }
    }

    /// Creates a stream of verified events starting from the latest position.
    ///
    /// # Arguments
    ///
    /// * `stream_id` - The address identifying the event stream (typically the package ID)
    ///
    /// # Returns
    ///
    /// A stream of `AuthenticatedEvent`s. Each event has been cryptographically verified against the
    /// EventStreamHead's MMR (Merkle Mountain Range) commitment.
    ///
    /// # Error Handling
    ///
    /// The stream automatically handles transient errors by retrying:
    /// - Network failures (`TransportError`)
    /// - Temporary RPC errors (e.g., `Unavailable`, `DeadlineExceeded`)
    ///
    /// The stream terminates on terminal errors:
    /// - `VerificationError`: Cryptographic verification failed (malicious or corrupted data)
    /// - `InternalError`: Invalid state or deserialization failures
    /// - Non-retriable RPC errors (e.g., `InvalidArgument`, `NotFound`)
    ///
    /// When the stream yields an `Err`, it's a terminal error and no more events will be produced.
    /// The client should stop consuming the stream when an error is received.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use std::sync::Arc;
    /// # use sui_light_client::authenticated_events::AuthenticatedEventsClient;
    /// # use sui_types::base_types::SuiAddress;
    /// # async fn example(client: Arc<AuthenticatedEventsClient>, stream_id: SuiAddress) {
    /// use futures::StreamExt;
    ///
    /// let mut stream = client.clone().stream_events(stream_id).await.unwrap();
    ///
    /// while let Some(result) = stream.next().await {
    ///     match result {
    ///         Ok(event) => {
    ///             println!("Verified event at checkpoint {}", event.checkpoint);
    ///         }
    ///         Err(e) => {
    ///             eprintln!("Terminal error: {:?}", e);
    ///             break;
    ///         }
    ///     }
    /// }
    /// # }
    /// ```
    pub async fn stream_events(
        self: Arc<Self>,
        stream_id: SuiAddress,
    ) -> Result<impl Stream<Item = Result<AuthenticatedEvent, ClientError>>, ClientError> {
        let config = self.config.clone();
        let stream_object_id = derive_event_stream_head_object_id(stream_id)
            .map_err(|e| ClientError::InternalError(e.to_string()))?;

        let result = self
            .fetch_current_stream_head_and_verify(stream_object_id)
            .await?;

        let (verified_head, start_checkpoint) = match result {
            Some((head, checkpoint)) => (Some(head), checkpoint + 1),
            None => (None, 0),
        };

        stream::create_event_stream_with_head(
            self,
            stream_id,
            stream_object_id,
            start_checkpoint,
            verified_head,
            config,
        )
        .await
    }

    /// Creates a stream of verified events resuming from after a specific checkpoint where
    /// event_stream_head was last modified.
    ///
    /// # Arguments
    ///
    /// * `stream_id` - The address identifying the event stream (typically the package ID)
    /// * `last_verified_checkpoint` - The checkpoint to resume from (exclusive). Must be a checkpoint
    ///   where the EventStreamHead was last modified (had events).
    ///
    /// # Returns
    ///
    /// A stream of `AuthenticatedEvent`s. Each event has been cryptographically verified against the
    /// EventStreamHead's MMR (Merkle Mountain Range) commitment.
    ///
    /// # Setup Errors
    ///
    /// Returns an error during stream creation if:
    /// - The EventStreamHead was not updated at `last_verified_checkpoint` (no events at that checkpoint)
    /// - The checkpoint has been pruned
    /// - Network or RPC communication errors during initial setup
    ///
    /// # Error Handling
    ///
    /// Once started, the stream automatically handles transient errors by retrying:
    /// - Network failures (`TransportError`)
    /// - Temporary RPC errors (e.g., `Unavailable`, `DeadlineExceeded`)
    ///
    /// The stream terminates on terminal errors:
    /// - `VerificationError`: Cryptographic verification failed (malicious or corrupted data)
    /// - `InternalError`: Invalid state or deserialization failures
    /// - Non-retriable RPC errors (e.g., `InvalidArgument`, `NotFound`)
    ///
    /// When the stream yields an `Err`, it's a terminal error and no more events will be produced.
    /// The client should stop consuming the stream when an error is received.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use std::sync::Arc;
    /// # use sui_light_client::authenticated_events::AuthenticatedEventsClient;
    /// # use sui_types::base_types::SuiAddress;
    /// # async fn example(client: Arc<AuthenticatedEventsClient>, stream_id: SuiAddress) {
    /// use futures::StreamExt;
    ///
    /// let last_checkpoint = 42;
    /// let mut stream = client.clone()
    ///     .stream_events_from_checkpoint(stream_id, last_checkpoint)
    ///     .await
    ///     .unwrap();
    ///
    /// while let Some(result) = stream.next().await {
    ///     match result {
    ///         Ok(event) => {
    ///             println!("Verified event at checkpoint {}", event.checkpoint);
    ///         }
    ///         Err(e) => {
    ///             eprintln!("Terminal error: {:?}", e);
    ///             break;
    ///         }
    ///     }
    /// }
    /// # }
    /// ```
    pub async fn stream_events_from_checkpoint(
        self: Arc<Self>,
        stream_id: SuiAddress,
        last_verified_checkpoint: u64,
    ) -> Result<impl Stream<Item = Result<AuthenticatedEvent, ClientError>>, ClientError> {
        let stream_object_id = derive_event_stream_head_object_id(stream_id)
            .map_err(|e| ClientError::InternalError(e.to_string()))?;

        let (verified_head, start_checkpoint) = if last_verified_checkpoint == 0 {
            (None, 0)
        } else {
            let verified_head = self
                .fetch_and_verify_stream_head(stream_object_id, last_verified_checkpoint)
                .await?;

            (Some(verified_head), last_verified_checkpoint + 1)
        };

        let config = self.config.clone();
        stream::create_event_stream_with_head(
            self,
            stream_id,
            stream_object_id,
            start_checkpoint,
            verified_head,
            config,
        )
        .await
    }

    /// Ratchet the trusted committee cache forward to (at least) `target_epoch` and return
    /// the committee for that epoch. Holds the cache lock for the entire ratchet so
    /// concurrent callers serialize on it.
    async fn ratchet_and_get_committee(
        &self,
        target_epoch: u64,
    ) -> Result<Arc<Committee>, ClientError> {
        let mut epoch_cache = self.epoch_cache.lock().await;

        if target_epoch > epoch_cache.current_epoch() {
            self.ratchet_locked(&mut epoch_cache, target_epoch).await?;
        }

        epoch_cache
            .get_committee_for_epoch(target_epoch)
            .ok_or_else(|| {
                ClientError::InternalError(format!(
                    "No committee in cache for epoch {} after ratcheting",
                    target_epoch,
                ))
            })
    }

    async fn ratchet_locked(
        &self,
        epoch_cache: &mut EpochCache,
        target_epoch: u64,
    ) -> Result<(), ClientError> {
        let starting_epoch = epoch_cache.current_epoch();

        let gap = target_epoch - starting_epoch;
        if gap > MAX_RATCHET_GAP {
            return Err(ClientError::InternalError(format!(
                "Refusing to trust ratchet across {} epochs (max allowed: {}). \
                 Current epoch {}, target epoch {}.",
                gap, MAX_RATCHET_GAP, starting_epoch, target_epoch,
            )));
        }

        tracing::debug!(
            "Trust ratcheting from epoch {} to {} ({} epochs)",
            starting_epoch,
            target_epoch,
            gap,
        );

        // fetch every epoch up to target_epoch, exclusive. each epoch's
        // final checkpoint contains the next epoch's committee, so we ratchet
        // up to epoch k by fetching up to k-1, inclusive.
        let epochs_to_fetch: Vec<u64> = (starting_epoch..target_epoch).collect();
        let checkpoint_data = self.parallel_fetch_epoch_data(&epochs_to_fetch).await?;

        for EpochCheckpointData {
            epoch,
            end_checkpoint_seq,
            summary,
        } in checkpoint_data
        {
            let current_committee = epoch_cache.current_committee().clone();

            summary
                .verify_with_contents(&current_committee, None)
                .map_err(|e| {
                    ClientError::VerificationError(format!(
                        "Failed to verify checkpoint {} for epoch {}: {}",
                        end_checkpoint_seq, epoch, e
                    ))
                })?;

            let next_committee = extract_new_committee_info(&summary).map_err(|e| {
                ClientError::VerificationError(format!(
                    "Failed to extract committee from checkpoint {} for epoch {}: {}",
                    end_checkpoint_seq, epoch, e
                ))
            })?;

            epoch_cache.apply_ratchet_update(next_committee);
        }

        Ok(())
    }

    async fn fetch_epoch_data(
        &self,
        epoch: u64,
    ) -> Result<Option<EpochCheckpointData>, ClientError> {
        let mut ledger_client = self.ledger_service.clone();

        let end_checkpoint_seq = match ledger_client.get_epoch(GetEpochRequest::new(epoch)).await {
            Ok(resp) => {
                let epoch_info = resp.into_inner().epoch.ok_or_else(|| {
                    ClientError::InternalError(format!(
                        "Failed to get epoch {}: Missing epoch info",
                        epoch,
                    ))
                })?;
                match epoch_info.last_checkpoint {
                    Some(end) => end,
                    None => return Ok(None),
                }
            }
            Err(status) if status.code() == tonic::Code::NotFound => return Ok(None),
            Err(status) => {
                return Err(ClientError::InternalError(format!(
                    "Failed to get epoch {}: {}",
                    epoch, status,
                )));
            }
        };

        let checkpoint_response = ledger_client
            .get_checkpoint(
                GetCheckpointRequest::by_sequence_number(end_checkpoint_seq)
                    .with_read_mask(FieldMask::from_paths(["summary.bcs", "signature"])),
            )
            .await
            .map_err(|status| {
                ClientError::InternalError(format!(
                    "Failed to fetch checkpoint {} for epoch {}: {}",
                    end_checkpoint_seq, epoch, status,
                ))
            })?
            .into_inner();

        let proto_checkpoint = checkpoint_response.checkpoint.ok_or_else(|| {
            ClientError::InternalError(format!(
                "Missing checkpoint in response for epoch {}",
                epoch,
            ))
        })?;

        let summary_data: sui_types::messages_checkpoint::CheckpointSummary = proto_checkpoint
            .summary()
            .bcs()
            .deserialize()
            .map_err(|e| {
                ClientError::InternalError(format!(
                    "Failed to deserialize summary for epoch {}: {}",
                    epoch, e,
                ))
            })?;

        let signature = sui_types::crypto::AuthorityStrongQuorumSignInfo::try_from(
            proto_checkpoint.signature(),
        )
        .map_err(|e| {
            ClientError::InternalError(format!(
                "Failed to convert signature for epoch {}: {:?}",
                epoch, e,
            ))
        })?;

        let summary =
            sui_types::messages_checkpoint::CertifiedCheckpointSummary::new_from_data_and_sig(
                summary_data,
                signature,
            );

        Ok(Some(EpochCheckpointData {
            epoch,
            end_checkpoint_seq,
            summary,
        }))
    }

    async fn parallel_fetch_epoch_data(
        &self,
        epochs: &[u64],
    ) -> Result<Vec<EpochCheckpointData>, ClientError> {
        use futures::StreamExt as _;

        let results: Vec<Result<Option<EpochCheckpointData>, ClientError>> =
            futures::stream::iter(epochs.iter().copied())
                .map(|epoch| self.fetch_epoch_data(epoch))
                .buffer_unordered(RATCHET_FETCH_CONCURRENCY)
                .collect()
                .await;

        let mut checkpoints = Vec::with_capacity(results.len());
        for result in results {
            if let Some(data) = result? {
                checkpoints.push(data);
            }
        }
        checkpoints.sort_by_key(|c| c.epoch);
        Ok(checkpoints)
    }

    pub(crate) fn event_service(&self) -> EventServiceClient<tonic::transport::Channel> {
        self.event_service.clone()
    }

    pub(crate) async fn fetch_and_verify_stream_head(
        &self,
        stream_object_id: sui_types::base_types::ObjectID,
        checkpoint: u64,
    ) -> Result<EventStreamHead, ClientError> {
        let mut proof_client = self.proof_service.clone();

        let mut request =
            sui_rpc_api::grpc::alpha::proof_service_proto::GetObjectInclusionProofRequest::default(
            );
        request.object_id = Some(stream_object_id.to_string());
        request.checkpoint = Some(checkpoint);

        let response = match proof_client.get_object_inclusion_proof(request).await {
            Ok(resp) => resp.into_inner(),
            Err(status) if status.code() == tonic::Code::FailedPrecondition => {
                return Err(ClientError::InternalError(format!(
                    "EventStreamHead was not updated at checkpoint {} (no events were emitted)",
                    checkpoint
                )));
            }
            Err(status) => return Err(ClientError::RpcError(status)),
        };

        let checkpoint_summary: sui_types::messages_checkpoint::CertifiedCheckpointSummary =
            bcs::from_bytes(response.checkpoint_summary.as_ref().ok_or_else(|| {
                ClientError::InternalError("Missing checkpoint summary".to_string())
            })?)?;
        let epoch = checkpoint_summary.epoch();

        let committee = self.ratchet_and_get_committee(epoch).await?;

        let object_data_bytes = response
            .object_data
            .as_ref()
            .ok_or_else(|| ClientError::InternalError("Missing object data".to_string()))?;

        let object: sui_types::object::Object = bcs::from_bytes(object_data_bytes)?;
        let stream_head = Self::extract_stream_head_from_object(&object)?;

        self.verify_ocs_inclusion_proof(&committee, &response)?;

        Ok(stream_head)
    }

    async fn fetch_current_stream_head_and_verify(
        &self,
        stream_object_id: sui_types::base_types::ObjectID,
    ) -> Result<Option<(EventStreamHead, u64)>, ClientError> {
        let mut ledger_client = self.ledger_service.clone();

        let mut request = sui_rpc_api::proto::sui::rpc::v2::GetObjectRequest::default();
        request.object_id = Some(stream_object_id.to_string());
        request.read_mask = Some(FieldMask::from_paths(["bcs"]));

        let response = match ledger_client.get_object(request).await {
            Ok(r) => r.into_inner(),
            Err(status) if status.code() == tonic::Code::NotFound => {
                return Ok(None);
            }
            Err(status) => return Err(ClientError::RpcError(status)),
        };

        let proto_object = response
            .object
            .ok_or_else(|| ClientError::InternalError("Missing object in response".to_string()))?;

        let bcs_data = proto_object
            .bcs
            .ok_or_else(|| ClientError::InternalError("Missing bcs data".to_string()))?;

        let object_data_bytes = bcs_data
            .value
            .ok_or_else(|| ClientError::InternalError("Missing bcs value".to_string()))?;
        let object: sui_types::object::Object = bcs::from_bytes(&object_data_bytes)?;
        let stream_head = Self::extract_stream_head_from_object(&object)?;
        let checkpoint = stream_head.checkpoint_seq;

        let verified_head = self
            .fetch_and_verify_stream_head(stream_object_id, checkpoint)
            .await?;

        Ok(Some((verified_head, checkpoint)))
    }

    fn verify_ocs_inclusion_proof(
        &self,
        committee: &Committee,
        response: &sui_rpc_api::grpc::alpha::proof_service_proto::GetObjectInclusionProofResponse,
    ) -> Result<(), ClientError> {
        let checkpoint_summary_bytes = response
            .checkpoint_summary
            .as_ref()
            .ok_or_else(|| ClientError::InternalError("Missing checkpoint summary".to_string()))?;

        let checkpoint_summary: sui_types::messages_checkpoint::CertifiedCheckpointSummary =
            bcs::from_bytes(checkpoint_summary_bytes)?;

        let object_ref_proto = response
            .object_ref
            .as_ref()
            .ok_or_else(|| ClientError::InternalError("Missing object_ref".to_string()))?;

        let inclusion_proof = response
            .inclusion_proof
            .as_ref()
            .ok_or_else(|| ClientError::InternalError("Missing inclusion proof".to_string()))?;

        let object_ref = converters::proto_object_ref_to_sui_object_ref(object_ref_proto)?;
        let ocs_inclusion_proof =
            converters::proto_ocs_inclusion_proof_to_light_client_proof(inclusion_proof)?;

        let target = OCSTarget::new_inclusion_target(object_ref);

        let proof = Proof {
            targets: ProofTarget::ObjectCheckpointState(target),
            checkpoint_summary,
            proof_contents: ProofContents::ObjectCheckpointStateProof(OCSProof::Inclusion(
                ocs_inclusion_proof,
            )),
        };

        proof.verify(committee)?;

        Ok(())
    }
}

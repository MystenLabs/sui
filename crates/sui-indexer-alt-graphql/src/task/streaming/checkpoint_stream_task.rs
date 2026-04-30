// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Checkpoint stream task.
//!
//! Maintains the connection to the fullnode's `SubscribeCheckpoints` gRPC
//! stream and broadcasts every checkpoint, in order, to GraphQL subscribers.
//!
//! ```text
//!   loop forever:
//!     connect (retry on transient errors, fail on auth/config)
//!     for each incoming checkpoint:
//!       if seq > last_broadcast + 1:
//!         recover_gap(last_broadcast + 1 ..= seq - 1)
//!       broadcast(checkpoint)
//!     on stream end: reconnect
//! ```
//!
//! # Reconnect
//!
//! gRPC connections drop for many reasons: rolling deploys, network blips,
//! server restarts. When the stream errors or closes, the task reconnects
//! with bounded exponential backoff. Transient errors retry indefinitely;
//! permanent gRPC errors (auth, config) terminate the task.
//!
//! # Gap recovery
//!
//! `SubscribeCheckpoints` resumes at the current tip on reconnect, not where
//! we left off. So after a drop, the first new message typically has a
//! sequence number well past the last we broadcast — checkpoints produced
//! during the drop are now invisible on the live stream.
//!
//! Detection happens on every incoming message: if `seq > last_broadcast + 1`,
//! the task fetches the missing checkpoints from kv-rpc (a `LedgerService`
//! endpoint serving historical checkpoints) and broadcasts them in order
//! before broadcasting the live message.
//!
//! ```text
//!   stream:    ... 10, 11   ╳ drop ╳   16, 17, 18 ...
//!                                       ↑ first after reconnect
//!
//!   on receive 16 (last_broadcast = 11):
//!     fetch 12..=15 from kv-rpc, broadcast 12..=15
//!     then broadcast 16
//! ```
//!
//! Recovery is chunked, with each chunk waiting for the kv-rpc indexer to
//! catch up. See `gap_recovery` for chunk size and retry semantics.
//!
//! # Subscriber view
//!
//! From a subscriber's perspective, the broadcast is strictly contiguous,
//! in-order, and contains no duplicates — there's no visible boundary
//! between recovered and live messages. Long disconnects converge naturally:
//! if recovery itself takes long enough that the validator advances past
//! the recovery target, the next live message is itself a gap, and detection
//! fires again.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use backoff::ExponentialBackoff;
use futures::StreamExt;
use move_core_types::account_address::AccountAddress;
use sui_futures::service::Service;
use sui_indexer_alt_reader::kv_loader::TransactionContents as NativeTransactionContents;
use sui_indexer_alt_reader::ledger_grpc_reader::LedgerGrpcReader;
use sui_package_resolver::Package;
use sui_rpc::field::FieldMask;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::Checkpoint as ProtoCheckpoint;
use sui_rpc::proto::sui::rpc::v2::ExecutedTransaction as ProtoExecutedTransaction;
use sui_rpc::proto::sui::rpc::v2::SubscribeCheckpointsRequest;
use sui_rpc::proto::sui::rpc::v2::SubscribeCheckpointsResponse;
use sui_rpc::proto::sui::rpc::v2::changed_object::OutputObjectState;
use sui_rpc::proto::sui::rpc::v2::subscription_service_client::SubscriptionServiceClient;
use sui_sdk_types::ValidatorAggregatedSignature;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;
use sui_types::crypto::AuthorityStrongQuorumSignInfo;
use sui_types::crypto::ToFromBytes;
use sui_types::effects::TransactionEffects;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::effects::TransactionEvents;
use sui_types::messages_checkpoint::CheckpointContents;
use sui_types::messages_checkpoint::CheckpointSummary;
use sui_types::object::Object as NativeObject;
use sui_types::signature::GenericSignature;
use sui_types::transaction::TransactionData;
use tokio::sync::broadcast;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::watch;
use tonic::Streaming;
use tonic::transport::Endpoint;
use tonic::transport::Uri;
use tracing::info;
use tracing::warn;

use crate::config::SubscriptionConfig;
use crate::scope::ExecutionObjectMap;
use crate::task::watermark::Watermarks;

use super::StreamingPackageStore;
use super::SubscriptionReadiness;
use super::gap_recovery::recover_gap;
use super::processed_checkpoint::ProcessedCheckpoint;
use super::processed_checkpoint::ProcessedTransaction;

// TODO: Make these configurable via SubscriptionConfig.
const MAX_GRPC_MESSAGE_SIZE_BYTES: usize = 128 * 1024 * 1024;
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(30);

/// A broadcast receiver used by subscription resolvers to receive processed checkpoints.
/// Stored in the GraphQL context; each subscriber calls `resubscribe()` to get its own
/// receiver. Using a Receiver (not Sender) ensures that when the stream task drops its
/// Sender, all subscribers receive `RecvError::Closed`.
pub(crate) type CheckpointBroadcaster = broadcast::Receiver<Arc<ProcessedCheckpoint>>;

/// Field mask requesting checkpoint-level and transaction-level fields needed by GraphQL resolvers.
pub(super) fn checkpoint_field_mask() -> FieldMask {
    FieldMask::from_paths([
        ProtoCheckpoint::path_builder().sequence_number(),
        ProtoCheckpoint::path_builder().summary().bcs().value(),
        ProtoCheckpoint::path_builder().signature().finish(),
        ProtoCheckpoint::path_builder().contents().bcs().value(),
        ProtoCheckpoint::path_builder()
            .transactions()
            .transaction()
            .finish(),
        ProtoCheckpoint::path_builder()
            .transactions()
            .effects()
            .finish(),
        ProtoCheckpoint::path_builder()
            .transactions()
            .events()
            .bcs()
            .value(),
        ProtoCheckpoint::path_builder()
            .transactions()
            .signatures()
            .bcs()
            .value(),
        ProtoCheckpoint::path_builder()
            .transactions()
            .balance_changes()
            .finish(),
        // Checkpoint-level objects (deduplicated across transactions).
        // Per-transaction `transactions.objects` is not populated in streamed checkpoints.
        ProtoCheckpoint::path_builder()
            .objects()
            .objects()
            .bcs()
            .value(),
    ])
}

/// Background service that connects to a fullnode's gRPC SubscribeCheckpoints endpoint,
/// processes incoming checkpoints, and broadcasts them to subscription resolvers.
pub(crate) struct CheckpointStreamTask {
    uri: Uri,
    sender: broadcast::Sender<Arc<ProcessedCheckpoint>>,
    broadcaster: CheckpointBroadcaster,
    streaming_packages: Arc<StreamingPackageStore>,
    package_eviction_tx: UnboundedSender<(u64, Vec<AccountAddress>)>,
    readiness: Arc<SubscriptionReadiness>,
    /// kv-rpc reader used to fill upstream gaps. Required: streaming subscriptions need a
    /// fallback source to recover from disconnects. lib.rs ensures this is configured when
    /// streaming is enabled.
    ledger_grpc_reader: LedgerGrpcReader,
    /// Watermarks receiver used by gap recovery to wait for kv-rpc to be at or past a target
    /// before fetching.
    watermarks_rx: watch::Receiver<Arc<Watermarks>>,
    gap_recovery_chunk_size: usize,
}

impl CheckpointStreamTask {
    pub(crate) fn new(
        uri: Uri,
        config: &SubscriptionConfig,
        streaming_packages: Arc<StreamingPackageStore>,
        package_eviction_tx: UnboundedSender<(u64, Vec<AccountAddress>)>,
        readiness: Arc<SubscriptionReadiness>,
        ledger_grpc_reader: LedgerGrpcReader,
        watermarks_rx: watch::Receiver<Arc<Watermarks>>,
    ) -> Self {
        let (sender, broadcaster) = broadcast::channel(config.broadcast_buffer);
        Self {
            uri,
            sender,
            broadcaster,
            streaming_packages,
            package_eviction_tx,
            readiness,
            ledger_grpc_reader,
            watermarks_rx,
            gap_recovery_chunk_size: config.gap_recovery_chunk_size,
        }
    }

    pub(crate) fn broadcaster(&self) -> CheckpointBroadcaster {
        self.broadcaster.resubscribe()
    }

    /// Connect to the fullnode's gRPC SubscribeCheckpoints endpoint.
    async fn connect(&self) -> anyhow::Result<Streaming<SubscribeCheckpointsResponse>> {
        let endpoint = Endpoint::from(self.uri.clone()).connect_timeout(CONNECTION_TIMEOUT);

        let mut client = SubscriptionServiceClient::connect(endpoint)
            .await
            .context("Failed to connect to checkpoint stream")?
            .max_decoding_message_size(MAX_GRPC_MESSAGE_SIZE_BYTES);

        let mut request = SubscribeCheckpointsRequest::default();
        request.read_mask = Some(checkpoint_field_mask());

        let stream = client
            .subscribe_checkpoints(request)
            .await
            .context("Failed to subscribe to checkpoint stream")?
            .into_inner();

        Ok(stream)
    }

    /// Drive the upstream checkpoint stream. Reconnects on any failure with bounded backoff,
    /// detects gaps on each incoming checkpoint, and fills them via kv-rpc before broadcasting
    /// the live message. Permanent connect errors (auth / config) terminate the task; transient
    /// errors retry indefinitely.
    pub(crate) fn run(self) -> Service {
        Service::new().spawn_aborting(async move {
            let mut last_broadcast: Option<u64> = None;
            let mut first_recorded = false;

            loop {
                info!("Connecting to checkpoint stream at {}...", self.uri);
                let stream = backoff::future::retry(reconnect_backoff(), || async {
                    self.connect().await.map_err(classify_connect_error)
                })
                .await?;
                info!("Connected to checkpoint stream at {}", self.uri);

                self.consume_stream(stream, &mut last_broadcast, &mut first_recorded)
                    .await?;
                warn!("Checkpoint stream ended, reconnecting");
            }
        })
    }

    /// Consume one connected stream until it ends or errors. On stream-level errors, logs
    /// and returns `Ok(())` so the caller reconnects. Per-message errors (malformed proto,
    /// gap recovery failure) propagate as `Err` and terminate the task.
    ///
    /// Generic over the stream type so tests can feed synthetic messages without standing
    /// up a real gRPC server. Tonic's `Streaming<T>` already implements `Stream`, so the
    /// production call site is unchanged.
    async fn consume_stream<S>(
        &self,
        mut stream: S,
        last_broadcast: &mut Option<u64>,
        first_recorded: &mut bool,
    ) -> anyhow::Result<()>
    where
        S: futures::Stream<Item = Result<SubscribeCheckpointsResponse, tonic::Status>> + Unpin,
    {
        while let Some(result) = stream.next().await {
            let response = match result {
                Ok(r) => r,
                Err(e) => {
                    warn!("Stream error: {e:#}");
                    return Ok(());
                }
            };

            let Some(checkpoint) = response.checkpoint else {
                continue;
            };
            let seq = checkpoint
                .sequence_number
                .context("Checkpoint without sequence_number")?;

            if !*first_recorded {
                self.readiness.record_first_checkpoint(seq);
                *first_recorded = true;
            }

            // Gap detection: synchronously fill any hole between last_broadcast and this
            // message before broadcasting the live cp.
            if let Some(last) = *last_broadcast
                && seq > last + 1
            {
                info!(from = last + 1, to = seq - 1, "Recovering gap");
                recover_gap(
                    &self.ledger_grpc_reader,
                    &self.watermarks_rx,
                    &self.sender,
                    last + 1,
                    seq - 1,
                    self.gap_recovery_chunk_size,
                )
                .await?;
            }

            self.index_and_broadcast(checkpoint, seq)?;
            *last_broadcast = Some(seq);
        }

        Ok(())
    }

    /// Index packages from the checkpoint into the streaming store, signal eviction,
    /// then process and broadcast. Indexing exposes the checkpoint's packages to
    /// subscribers immediately, ahead of `kv_packages`. Recovered checkpoints from
    /// `recover_gap` skip this step because the gate already waits for both
    /// `ledger_grpc` and `kv_packages` to catch up.
    fn index_and_broadcast(&self, checkpoint: ProtoCheckpoint, seq: u64) -> anyhow::Result<()> {
        let packages = extract_packages(&checkpoint);
        if !packages.is_empty() {
            self.streaming_packages.index_packages(seq, &packages);
            let ids = packages.iter().map(|p| p.storage_id()).collect();
            // Send errors only if the eviction task has exited; nothing will drain the
            // store, but we keep serving.
            let _ = self.package_eviction_tx.send((seq, ids));
        }
        let processed = process_checkpoint(checkpoint)?;
        // Ignore send errors: no active subscribers is a normal state.
        let _ = self.sender.send(Arc::new(processed));
        Ok(())
    }
}

/// Backoff between reconnect attempts: 1s growing to 5s max, no overall deadline.
/// Tight cap so we recover quickly from rolling deploys; infinite retry so the streaming
/// server stays alive through extended outages.
fn reconnect_backoff() -> ExponentialBackoff {
    ExponentialBackoff {
        initial_interval: Duration::from_secs(1),
        max_interval: Duration::from_secs(5),
        max_elapsed_time: None,
        ..Default::default()
    }
}

/// Classify connect errors as transient or permanent. gRPC codes that signal config problems
/// (auth, missing endpoint, malformed request) terminate the task; everything else retries.
fn classify_connect_error(e: anyhow::Error) -> backoff::Error<anyhow::Error> {
    use tonic::Code;
    let permanent = e.downcast_ref::<tonic::Status>().is_some_and(|s| {
        matches!(
            s.code(),
            Code::Unauthenticated
                | Code::PermissionDenied
                | Code::InvalidArgument
                | Code::Unimplemented
        )
    });

    if permanent {
        warn!("Permanent connect failure (config / auth issue): {e:#}");
        backoff::Error::permanent(e)
    } else {
        warn!("Connect failed, will retry: {e:#}");
        backoff::Error::transient(e)
    }
}

pub(super) fn process_checkpoint(
    checkpoint: ProtoCheckpoint,
) -> anyhow::Result<ProcessedCheckpoint> {
    let sequence_number = checkpoint
        .sequence_number
        .context("Checkpoint without sequence_number")?;

    let summary: CheckpointSummary = checkpoint
        .summary
        .as_ref()
        .and_then(|s| s.bcs.as_ref())
        .context("Missing summary.bcs")?
        .deserialize()
        .context("Failed to deserialize checkpoint summary")?;

    let contents: CheckpointContents = checkpoint
        .contents
        .as_ref()
        .and_then(|c| c.bcs.as_ref())
        .context("Missing contents.bcs")?
        .deserialize()
        .context("Failed to deserialize checkpoint contents")?;

    let signature: AuthorityStrongQuorumSignInfo = {
        let sdk_sig = ValidatorAggregatedSignature::try_from(
            checkpoint.signature.as_ref().context("Missing signature")?,
        )
        .context("Failed to parse checkpoint signature")?;
        AuthorityStrongQuorumSignInfo::from(sdk_sig)
    };

    let timestamp_ms = summary.timestamp_ms;
    let cp_sequence_number = sequence_number;
    let tx_lo = summary.network_total_transactions - checkpoint.transactions.len() as u64;
    let checkpoint_objects = deserialize_checkpoint_objects(&checkpoint)?;
    let transactions = checkpoint
        .transactions
        .iter()
        .enumerate()
        .map(|(i, proto)| {
            process_transaction(
                proto,
                &checkpoint_objects,
                timestamp_ms,
                cp_sequence_number,
                tx_lo + i as u64,
            )
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    Ok(ProcessedCheckpoint {
        sequence_number,
        summary,
        contents,
        signature,
        transactions,
    })
}

fn process_transaction(
    proto: &ProtoExecutedTransaction,
    checkpoint_objects: &BTreeMap<(ObjectID, SequenceNumber), NativeObject>,
    timestamp_ms: u64,
    cp_sequence_number: u64,
    tx_sequence_number: u64,
) -> anyhow::Result<ProcessedTransaction> {
    let transaction_data: TransactionData = proto
        .transaction
        .as_ref()
        .and_then(|t| t.bcs.as_ref())
        .context("Missing transaction.bcs")?
        .deserialize()
        .context("Failed to deserialize transaction data")?;

    let effects: TransactionEffects = proto
        .effects
        .as_ref()
        .and_then(|e| e.bcs.as_ref())
        .context("Missing effects.bcs")?
        .deserialize()
        .context("Failed to deserialize transaction effects")?;

    let events = proto
        .events
        .as_ref()
        .and_then(|e| e.bcs.as_ref())
        .map(|bcs| {
            bcs.deserialize()
                .context("Failed to deserialize transaction events")
        })
        .transpose()?
        .map(|e: TransactionEvents| e.data);

    let signatures: Vec<GenericSignature> = proto
        .signatures
        .iter()
        .map(|sig| {
            let bytes = sig
                .bcs
                .as_ref()
                .context("Missing signature bcs")?
                .value
                .as_deref()
                .unwrap_or(&[]);
            GenericSignature::from_bytes(bytes)
                .map_err(|e| anyhow::anyhow!("Failed to parse user signature: {e}"))
        })
        .collect::<anyhow::Result<_>>()?;

    let digest = *effects.transaction_digest();

    let execution_objects = build_execution_objects(checkpoint_objects, proto)?;

    let contents = NativeTransactionContents::ExecutedTransaction(
        sui_indexer_alt_reader::kv_loader::ExecutedTransactionData {
            effects: Box::new(effects),
            events,
            transaction_data: Box::new(transaction_data),
            signatures,
            balance_changes: proto.balance_changes.clone(),
            proto_effects: proto.effects.clone(),
            proto_transaction: proto.transaction.clone(),
            timestamp_ms: Some(timestamp_ms),
            cp_sequence_number: Some(cp_sequence_number),
        },
    );

    Ok(ProcessedTransaction {
        tx_sequence_number,
        digest,
        contents,
        execution_objects,
    })
}

/// Extract package objects from a streamed checkpoint. Returned packages will be
/// inserted into the streaming index and queued for eventual eviction.
fn extract_packages(checkpoint: &ProtoCheckpoint) -> Vec<Arc<Package>> {
    let Some(object_set) = &checkpoint.objects else {
        return Vec::new();
    };
    let mut packages = Vec::new();
    for obj in &object_set.objects {
        let Some(bcs) = &obj.bcs else { continue };
        let Ok(native_obj) = bcs.deserialize::<NativeObject>() else {
            continue;
        };
        let Some(move_package) = native_obj.data.try_as_package() else {
            continue;
        };
        let Ok(package) = Package::read_from_package(move_package) else {
            continue;
        };
        packages.push(Arc::new(package));
    }
    packages
}

/// Deserialize all objects from the checkpoint-level ObjectSet.
fn deserialize_checkpoint_objects(
    checkpoint: &ProtoCheckpoint,
) -> anyhow::Result<BTreeMap<(ObjectID, SequenceNumber), NativeObject>> {
    let mut map = BTreeMap::new();
    if let Some(object_set) = &checkpoint.objects {
        for obj in &object_set.objects {
            if let Some(bcs) = &obj.bcs {
                let native_obj: NativeObject = bcs
                    .deserialize()
                    .context("Failed to deserialize checkpoint object BCS")?;
                map.insert((native_obj.id(), native_obj.version()), native_obj);
            }
        }
    }
    Ok(map)
}

/// Build an ExecutionObjectMap for a single transaction by filtering checkpoint-level objects
/// using the transaction's `changed_objects` from effects.
///
/// Includes both input objects (previous version) and output objects (new version) so that
/// both `inputState` and `outputState` can resolve from streaming.
/// Objects with `DoesNotExist` output state become tombstones (None).
fn build_execution_objects(
    checkpoint_objects: &BTreeMap<(ObjectID, SequenceNumber), NativeObject>,
    proto_tx: &ProtoExecutedTransaction,
) -> anyhow::Result<ExecutionObjectMap> {
    let mut map = BTreeMap::new();

    if let Some(effects) = &proto_tx.effects {
        let lamport_version = SequenceNumber::from_u64(
            effects
                .lamport_version
                .context("Effects should have lamport_version")?,
        );

        for changed_obj in &effects.changed_objects {
            let object_id: ObjectID = changed_obj
                .object_id
                .as_ref()
                .and_then(|id| id.parse().ok())
                .context("ChangedObject should have valid object_id")?;

            // Input object (previous version, before the transaction).
            if let Some(input_version) = changed_obj.input_version {
                let input_version = SequenceNumber::from_u64(input_version);
                if let Some(obj) = checkpoint_objects.get(&(object_id, input_version)) {
                    map.insert((object_id, input_version), Some(obj.clone()));
                }
            }

            // Output object (new version, after the transaction).
            if changed_obj.output_state() == OutputObjectState::DoesNotExist {
                map.insert((object_id, lamport_version), None);
            } else if let Some(obj) = checkpoint_objects.get(&(object_id, lamport_version)) {
                map.insert((object_id, lamport_version), Some(obj.clone()));
            }
        }
    }

    Ok(Arc::new(map))
}

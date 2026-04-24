// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use futures::StreamExt;
use move_core_types::account_address::AccountAddress;
use sui_futures::service::Service;
use sui_indexer_alt_reader::kv_loader::TransactionContents as NativeTransactionContents;
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
use tonic::Streaming;
use tonic::transport::Endpoint;
use tonic::transport::Uri;
use tracing::info;

use crate::config::SubscriptionConfig;
use crate::scope::ExecutionObjectMap;

use super::StreamingPackageStore;
use super::SubscriptionReadiness;
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
fn checkpoint_field_mask() -> FieldMask {
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
}

impl CheckpointStreamTask {
    pub(crate) fn new(
        uri: Uri,
        config: &SubscriptionConfig,
        streaming_packages: Arc<StreamingPackageStore>,
        package_eviction_tx: UnboundedSender<(u64, Vec<AccountAddress>)>,
        readiness: Arc<SubscriptionReadiness>,
    ) -> Self {
        let (sender, broadcaster) = broadcast::channel(config.broadcast_buffer);
        Self {
            uri,
            sender,
            broadcaster,
            streaming_packages,
            package_eviction_tx,
            readiness,
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

    // TODO(Phase 2): Connection and stream errors currently terminate the task.
    // Proper error handling will be addressed alongside backfilling.
    pub(crate) fn run(self) -> Service {
        Service::new().spawn_aborting(async move {
            info!("Connecting to checkpoint stream at {}...", self.uri);
            let mut stream = self.connect().await?;
            info!("Connected to checkpoint stream at {}", self.uri);

            let mut first_checkpoint_recorded = false;
            while let Some(result) = stream.next().await {
                let response = result.context("Checkpoint stream error")?;
                if let Some(checkpoint) = response.checkpoint {
                    let sequence_number = checkpoint
                        .sequence_number
                        .context("Checkpoint without sequence_number")?;
                    if !first_checkpoint_recorded {
                        self.readiness.record_first_checkpoint(sequence_number);
                        first_checkpoint_recorded = true;
                    }
                    let packages = extract_packages(&checkpoint);
                    if !packages.is_empty() {
                        self.streaming_packages
                            .index_packages(sequence_number, &packages);
                        let ids = packages.iter().map(|p| p.storage_id()).collect();
                        // Send errors only if the eviction task has exited — at that
                        // point nothing will drain the store, but we keep serving.
                        let _ = self.package_eviction_tx.send((sequence_number, ids));
                    }
                    let processed = process_checkpoint(checkpoint)?;
                    // Ignore send errors — no active subscribers is a normal state
                    // (e.g., no clients have connected yet). The checkpoint is simply dropped.
                    let _ = self.sender.send(Arc::new(processed));
                }
            }

            Ok(())
        })
    }
}

fn process_checkpoint(checkpoint: ProtoCheckpoint) -> anyhow::Result<ProcessedCheckpoint> {
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

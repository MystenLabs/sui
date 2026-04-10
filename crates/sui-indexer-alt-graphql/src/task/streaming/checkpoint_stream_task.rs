// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use futures::StreamExt;
use sui_futures::service::Service;
use sui_indexer_alt_reader::kv_loader::TransactionContents as NativeTransactionContents;
use sui_rpc::field::FieldMask;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::Checkpoint as ProtoCheckpoint;
use sui_rpc::proto::sui::rpc::v2::ExecutedTransaction as ProtoExecutedTransaction;
use sui_rpc::proto::sui::rpc::v2::SubscribeCheckpointsRequest;
use sui_rpc::proto::sui::rpc::v2::SubscribeCheckpointsResponse;
use sui_rpc::proto::sui::rpc::v2::subscription_service_client::SubscriptionServiceClient;
use sui_sdk_types::ValidatorAggregatedSignature;
use sui_types::crypto::AuthorityStrongQuorumSignInfo;
use sui_types::crypto::ToFromBytes;
use sui_types::effects::TransactionEffects;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::effects::TransactionEvents;
use sui_types::messages_checkpoint::CheckpointContents;
use sui_types::messages_checkpoint::CheckpointSummary;
use sui_types::signature::GenericSignature;
use sui_types::transaction::TransactionData;
use tokio::sync::broadcast;
use tonic::Streaming;
use tonic::transport::Endpoint;
use tonic::transport::Uri;
use tracing::info;

use crate::config::SubscriptionConfig;

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
    ])
}

/// Background service that connects to a fullnode's gRPC SubscribeCheckpoints endpoint,
/// processes incoming checkpoints, and broadcasts them to subscription resolvers.
pub(crate) struct CheckpointStreamTask {
    uri: Uri,
    sender: broadcast::Sender<Arc<ProcessedCheckpoint>>,
    broadcaster: CheckpointBroadcaster,
}

impl CheckpointStreamTask {
    pub(crate) fn new(uri: Uri, config: &SubscriptionConfig) -> Self {
        let (sender, broadcaster) = broadcast::channel(config.broadcast_buffer);
        Self {
            uri,
            sender,
            broadcaster,
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

            while let Some(result) = stream.next().await {
                let response = result.context("Checkpoint stream error")?;
                if let Some(checkpoint) = response.checkpoint {
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
    let transactions = checkpoint
        .transactions
        .iter()
        .enumerate()
        .map(|(i, proto)| {
            process_transaction(proto, timestamp_ms, cp_sequence_number, tx_lo + i as u64)
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
    })
}

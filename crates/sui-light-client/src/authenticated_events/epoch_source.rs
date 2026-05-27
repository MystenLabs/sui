// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Fetching end-of-epoch checkpoint summaries used for trust ratcheting.
//!
//! Both the archival and fullnode endpoints expose the same
//! [`LedgerService`] gRPC API.
//!
//! [`EpochDataFetcher`] composes two of them: archive-first, fullnode for
//! the rest. A miss from the archive falls through to the fullnode.

use super::ClientError;
use futures::StreamExt;
use std::time::Duration;
use sui_rpc::field::{FieldMask, FieldMaskUtil};
use sui_rpc_api::proto::sui::rpc::v2::ledger_service_client::LedgerServiceClient;
use sui_rpc_api::proto::sui::rpc::v2::{GetCheckpointRequest, GetEpochRequest};
use tonic::transport::Channel;

/// Max in-flight epoch fetches against the archive endpoint.
const ARCHIVE_CONCURRENCY: usize = 4;

/// Max in-flight epoch fetches against the fullnode endpoint.
const FULLNODE_CONCURRENCY: usize = 4;

const MAX_RPC_RETRIES: u32 = 5;
const BASE_RETRY_DELAY_MS: u64 = 250;
const MAX_RETRY_DELAY_MS: u64 = 4_000;
const MAX_JITTER_MS: u64 = 500;

pub(crate) struct EpochCheckpointData {
    pub epoch: u64,
    pub end_checkpoint_seq: u64,
    pub summary: sui_types::messages_checkpoint::CertifiedCheckpointSummary,
}

/// Coordinates end-of-epoch data retrieval across the archive and fullnode sources.
pub(crate) struct EpochDataFetcher {
    archive: Option<GrpcEpochSource>,
    fullnode: GrpcEpochSource,
}

impl EpochDataFetcher {
    pub(crate) fn new(
        fullnode: LedgerServiceClient<Channel>,
        archive: Option<LedgerServiceClient<Channel>>,
    ) -> Self {
        Self {
            archive: archive.map(|ledger_service| GrpcEpochSource {
                ledger_service,
                concurrency: ARCHIVE_CONCURRENCY,
            }),
            fullnode: GrpcEpochSource {
                ledger_service: fullnode,
                concurrency: FULLNODE_CONCURRENCY,
            },
        }
    }

    /// Fetch end-of-epoch data for every epoch in `epochs`. Results are sorted by
    /// epoch. Archive is tried first; epochs the archive doesn't yet cover (or
    /// errored on) fall through to the fullnode.
    pub(crate) async fn fetch_many(
        &self,
        epochs: &[u64],
    ) -> Result<Vec<EpochCheckpointData>, ClientError> {
        let mut fetched = Vec::with_capacity(epochs.len());
        let mut pending: Vec<u64> = epochs.to_vec();

        if let Some(archive) = &self.archive {
            let results = archive.fetch_batch(pending).await;
            pending = Vec::new();
            for (epoch, result) in results {
                match result {
                    Ok(Some(data)) => fetched.push(data),
                    Ok(None) => pending.push(epoch),
                    Err(e) => {
                        tracing::warn!(
                            epoch,
                            error = %e,
                            "archive fetch failed; retrying via fullnode",
                        );
                        pending.push(epoch);
                    }
                }
            }
        }

        if !pending.is_empty() {
            let results = self.fullnode.fetch_batch(pending).await;
            for (_, result) in results {
                if let Some(data) = result? {
                    fetched.push(data);
                }
            }
        }

        fetched.sort_by_key(|d| d.epoch);
        Ok(fetched)
    }
}

struct GrpcEpochSource {
    ledger_service: LedgerServiceClient<Channel>,
    concurrency: usize,
}

impl GrpcEpochSource {
    /// `Ok(None)` means this endpoint can't serve the epoch — either it
    /// returned `NotFound` or it knows of the epoch but doesn't have its
    /// end-of-epoch checkpoint (too recent on the archive, too old on a
    /// pruning fullnode). Hard errors are propagated.
    async fn fetch_one(&self, epoch: u64) -> Result<Option<EpochCheckpointData>, ClientError> {
        let epoch_response = match retry_rpc(epoch, || {
            let mut ledger_client = self.ledger_service.clone();
            async move {
                ledger_client
                    .get_epoch(
                        GetEpochRequest::new(epoch)
                            .with_read_mask(FieldMask::from_paths(["last_checkpoint"])),
                    )
                    .await
            }
        })
        .await
        {
            Ok(resp) => resp.into_inner(),
            Err(status) if status.code() == tonic::Code::NotFound => return Ok(None),
            Err(status) => return Err(ClientError::RpcError(status)),
        };

        let epoch_info = epoch_response.epoch.ok_or_else(|| {
            ClientError::InternalError(
                format!("Failed to get epoch {}: Missing epoch info", epoch,),
            )
        })?;
        let Some(end_checkpoint_seq) = epoch_info.last_checkpoint else {
            return Ok(None);
        };

        let checkpoint_response = retry_rpc(epoch, || {
            let mut ledger_client = self.ledger_service.clone();
            async move {
                ledger_client
                    .get_checkpoint(
                        GetCheckpointRequest::by_sequence_number(end_checkpoint_seq)
                            .with_read_mask(FieldMask::from_paths(["summary.bcs", "signature"])),
                    )
                    .await
            }
        })
        .await
        .map_err(ClientError::RpcError)?
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

    async fn fetch_batch(
        &self,
        epochs: Vec<u64>,
    ) -> Vec<(u64, Result<Option<EpochCheckpointData>, ClientError>)> {
        futures::stream::iter(epochs)
            .map(|epoch| async move { (epoch, self.fetch_one(epoch).await) })
            .buffer_unordered(self.concurrency)
            .collect()
            .await
    }
}

async fn retry_rpc<T, F, Fut>(jitter_seed: u64, mut f: F) -> Result<T, tonic::Status>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, tonic::Status>>,
{
    let mut attempt: u32 = 0;
    loop {
        match f().await {
            Ok(v) => return Ok(v),
            Err(status)
                if attempt < MAX_RPC_RETRIES
                    && ClientError::is_retriable_grpc_code(status.code()) =>
            {
                let exp = BASE_RETRY_DELAY_MS
                    .saturating_mul(1u64 << attempt)
                    .min(MAX_RETRY_DELAY_MS);
                let jitter = jitter_seed % MAX_JITTER_MS;
                tracing::warn!(
                    attempt,
                    code = ?status.code(),
                    "retriable RPC error, backing off {}ms: {}",
                    exp + jitter,
                    status.message(),
                );
                tokio::time::sleep(Duration::from_millis(exp + jitter)).await;
                attempt += 1;
            }
            Err(status) => return Err(status),
        }
    }
}

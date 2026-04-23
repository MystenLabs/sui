// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Fetching end-of-epoch checkpoint summaries used for trust ratcheting.
//!
//! Two sources are supported:
//!
//! - [`ArchiveEpochSource`]: downloads compressed checkpoints from the public
//!   `checkpoints.{network}.sui.io` archive. Has complete history, isn't rate
//!   limited, and uses an `epochs.json` index to map epoch -> end-of-epoch
//!   checkpoint sequence number.
//! - [`FullnodeEpochSource`]: queries the gRPC ledger service for recent epochs
//!   the archive doesn't yet cover. Rate-limited on public endpoints.
//!
//! [`EpochDataFetcher`] composes both: archive-first for any epoch within
//! archive coverage, fullnode for the rest or for archive failures.

use super::ClientError;
use futures::StreamExt;
use std::sync::Arc;
use std::time::Duration;
use sui_rpc::field::{FieldMask, FieldMaskUtil};
use sui_rpc_api::proto::sui::rpc::v2::ledger_service_client::LedgerServiceClient;
use sui_rpc_api::proto::sui::rpc::v2::{GetCheckpointRequest, GetEpochRequest};
use sui_storage::object_store::util::{end_of_epoch_data, fetch_checkpoint};
use tonic::transport::Channel;

/// Max concurrent checkpoint fetches from the public archive. Archive is served
/// from object storage / CDN and tolerates high concurrency.
const ARCHIVE_CONCURRENCY: usize = 20;

/// Max concurrent checkpoint fetches from the fullnode. Public fullnodes rate
/// limit bursts, so this must stay well below anonymous QPS thresholds.
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
    archive: Option<ArchiveEpochSource>,
    fullnode: FullnodeEpochSource,
}

impl EpochDataFetcher {
    pub(crate) fn new(
        ledger_service: LedgerServiceClient<Channel>,
        archive_url: Option<&str>,
    ) -> Result<Self, ClientError> {
        let archive = archive_url.map(ArchiveEpochSource::new).transpose()?;
        Ok(Self {
            archive,
            fullnode: FullnodeEpochSource::new(ledger_service),
        })
    }

    /// Fetch end-of-epoch data for every epoch in `epochs`. Results are sorted by
    /// epoch. Epochs covered by the archive are fetched there first; failures and
    /// epochs beyond archive coverage fall through to the fullnode.
    pub(crate) async fn fetch_many(
        &self,
        epochs: &[u64],
    ) -> Result<Vec<EpochCheckpointData>, ClientError> {
        let mut fetched = Vec::with_capacity(epochs.len());
        let mut pending: Vec<u64> = epochs.to_vec();

        if let Some(archive) = &self.archive {
            let (archive_eligible, rest) = archive.partition(pending).await?;
            pending = rest;

            let archive_results = archive.fetch_batch(archive_eligible).await;
            for (epoch, result) in archive_results {
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

        let fullnode_results = self.fullnode.fetch_batch(pending).await?;
        fetched.extend(fullnode_results);

        fetched.sort_by_key(|d| d.epoch);
        Ok(fetched)
    }
}

struct ArchiveEpochSource {
    store: Arc<dyn object_store::ObjectStore>,
    url: String,
    // Cached epoch -> end-of-epoch checkpoint seq (indexed by epoch). Fetched
    // once per client lifetime on first partition/fetch call.
    epoch_map: tokio::sync::OnceCell<Vec<u64>>,
}

impl ArchiveEpochSource {
    fn new(url: &str) -> Result<Self, ClientError> {
        let parsed = url::Url::parse(url).map_err(|e| {
            ClientError::InternalError(format!("invalid archive URL {}: {}", url, e))
        })?;
        let (store, _) = object_store::parse_url(&parsed).map_err(|e| {
            ClientError::InternalError(format!("archive store init failed for {}: {}", url, e,))
        })?;
        Ok(Self {
            store: Arc::from(store),
            url: url.to_string(),
            epoch_map: tokio::sync::OnceCell::new(),
        })
    }

    async fn epoch_map(&self) -> Result<&Vec<u64>, ClientError> {
        let url = self.url.clone();
        self.epoch_map
            .get_or_try_init(|| async move {
                end_of_epoch_data(&url, vec![]).await.map_err(|e| {
                    ClientError::InternalError(format!(
                        "failed to fetch epochs.json from archive: {}",
                        e,
                    ))
                })
            })
            .await
    }

    // Split into (archive-eligible, beyond-coverage). Caller runs `fetch_batch` on
    // the first half and forwards the rest to the next source.
    async fn partition(&self, epochs: Vec<u64>) -> Result<(Vec<u64>, Vec<u64>), ClientError> {
        let coverage = self.epoch_map().await?.len() as u64;
        Ok(epochs.into_iter().partition(|&e| e < coverage))
    }

    async fn fetch_one(&self, epoch: u64) -> Result<Option<EpochCheckpointData>, ClientError> {
        let epoch_map = self.epoch_map().await?;
        let Some(&end_checkpoint_seq) = epoch_map.get(epoch as usize) else {
            return Ok(None);
        };

        let checkpoint = fetch_checkpoint(&self.store, end_checkpoint_seq)
            .await
            .map_err(|e| {
                ClientError::InternalError(format!(
                    "archive fetch_checkpoint({}) for epoch {} failed: {}",
                    end_checkpoint_seq, epoch, e,
                ))
            })?;

        Ok(Some(EpochCheckpointData {
            epoch,
            end_checkpoint_seq,
            summary: checkpoint.summary,
        }))
    }

    async fn fetch_batch(
        &self,
        epochs: Vec<u64>,
    ) -> Vec<(u64, Result<Option<EpochCheckpointData>, ClientError>)> {
        futures::stream::iter(epochs)
            .map(|epoch| async move { (epoch, self.fetch_one(epoch).await) })
            .buffer_unordered(ARCHIVE_CONCURRENCY)
            .collect()
            .await
    }
}

struct FullnodeEpochSource {
    ledger_service: LedgerServiceClient<Channel>,
}

impl FullnodeEpochSource {
    fn new(ledger_service: LedgerServiceClient<Channel>) -> Self {
        Self { ledger_service }
    }

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
        let end_checkpoint_seq = epoch_info.last_checkpoint.ok_or_else(|| {
            // The fullnode omits `last_checkpoint` when its end-of-epoch checkpoint
            // has been pruned. Without that summary we can't verify the committee
            // transition — caller must supply an archive URL for pre-pruning epochs.
            ClientError::InternalError(format!(
                "fullnode does not expose last_checkpoint for epoch {} (likely pruned)",
                epoch,
            ))
        })?;

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

    async fn fetch_batch(&self, epochs: Vec<u64>) -> Result<Vec<EpochCheckpointData>, ClientError> {
        if epochs.is_empty() {
            return Ok(Vec::new());
        }

        let results: Vec<Result<Option<EpochCheckpointData>, ClientError>> =
            futures::stream::iter(epochs)
                .map(|epoch| self.fetch_one(epoch))
                .buffer_unordered(FULLNODE_CONCURRENCY)
                .collect()
                .await;

        let mut out = Vec::with_capacity(results.len());
        for result in results {
            if let Some(data) = result? {
                out.push(data);
            }
        }
        Ok(out)
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

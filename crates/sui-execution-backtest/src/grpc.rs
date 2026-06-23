// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Thin gRPC client over a Sui fullnode, used for the two things the checkpoint stream does not
//! already provide: resolving an epoch to its checkpoint range + protocol version, and fetching
//! package objects that are not present in a checkpoint's object set.

use std::str::FromStr as _;

use anyhow::{Context as _, Result};
use sui_rpc::field::{FieldMask, FieldMaskUtil};
use sui_rpc::proto::sui::rpc::v2 as rpc;
use sui_types::base_types::ObjectID;
use sui_types::digests::{ChainIdentifier, CheckpointDigest};
use sui_types::object::Object;
use tracing::warn;
use url::Url;

/// Maximum retry attempts for a transient gRPC failure (notably HTTP 429 rate-limiting).
const MAX_RETRIES: u32 = 8;

/// The checkpoint range and protocol parameters for a single epoch.
#[derive(Clone, Copy, Debug)]
pub struct EpochBounds {
    pub first_checkpoint: u64,
    pub last_checkpoint: u64,
    pub protocol_version: u64,
    pub reference_gas_price: u64,
}

#[derive(Clone)]
pub struct RpcClient {
    client: sui_rpc::client::Client,
}

impl RpcClient {
    pub fn new(url: Url) -> Result<Self> {
        Ok(Self {
            client: sui_rpc::client::Client::new(url.to_string())
                .context("Failed to construct gRPC client")?,
        })
    }

    /// Resolve `epoch` to its first/last checkpoint, protocol version, and reference gas price.
    pub async fn epoch_bounds(&self, epoch: u64) -> Result<EpochBounds> {
        let mut ledger = self.client.clone().ledger_client();
        let request = rpc::GetEpochRequest::new(epoch).with_read_mask(FieldMask::from_paths([
            "first_checkpoint",
            "last_checkpoint",
            "protocol_config.protocol_version",
            "reference_gas_price",
        ]));
        let response = ledger
            .get_epoch(request)
            .await
            .with_context(|| format!("get_epoch({epoch}) failed"))?
            .into_inner();
        let epoch_msg = response
            .epoch
            .with_context(|| format!("get_epoch({epoch}) returned no epoch"))?;
        let protocol_version = epoch_msg
            .protocol_config
            .as_ref()
            .and_then(|c| c.protocol_version)
            .with_context(|| format!("epoch {epoch} missing protocol_version"))?;
        Ok(EpochBounds {
            first_checkpoint: epoch_msg
                .first_checkpoint
                .with_context(|| format!("epoch {epoch} missing first_checkpoint"))?,
            last_checkpoint: epoch_msg
                .last_checkpoint
                .with_context(|| format!("epoch {epoch} missing last_checkpoint"))?,
            protocol_version,
            reference_gas_price: epoch_msg.reference_gas_price.unwrap_or(0),
        })
    }

    /// The network's chain identifier, via the fullnode's `GetServiceInfo` (cheap). The remote
    /// object store would otherwise derive this by fetching + decoding genesis (checkpoint 0), a
    /// large, slow, one-time startup cost; this avoids it (see [`crate::ingestion`]).
    pub async fn chain_id(&self) -> Result<ChainIdentifier> {
        let mut ledger = self.client.clone().ledger_client();
        let response = ledger
            .get_service_info(rpc::GetServiceInfoRequest::default())
            .await
            .context("get_service_info failed")?
            .into_inner();
        Ok(CheckpointDigest::from_str(response.chain_id())?.into())
    }

    /// Fetch a package object by id (immutable, so the latest version is what executed against it).
    /// Returns `None` if the fullnode does not have it. Retries transient errors (notably HTTP 429
    /// rate-limiting) with exponential backoff, because a dropped package fetch would otherwise
    /// surface as a spurious execution failure rather than a real result.
    pub async fn fetch_object(&self, id: ObjectID) -> Result<Option<Object>> {
        let mut attempt = 0u32;
        let response = loop {
            let mut ledger = self.client.clone().ledger_client();
            let request = rpc::GetObjectRequest::new(&id.into_bytes().into())
                .with_read_mask(FieldMask::from_paths(["bcs"]));
            match ledger.get_object(request).await {
                Ok(response) => break response.into_inner(),
                Err(status) if attempt < MAX_RETRIES && is_transient(&status) => {
                    tokio::time::sleep(backoff_delay(attempt, id.into_bytes()[0])).await;
                    attempt = attempt.saturating_add(1);
                }
                Err(status) => {
                    return Err(anyhow::anyhow!(status))
                        .with_context(|| format!("get_object({id}) failed"));
                }
            }
        };
        let Some(bytes) = response.object.and_then(|o| o.bcs).and_then(|b| b.value) else {
            return Ok(None);
        };
        let object: Object =
            bcs::from_bytes(&bytes).with_context(|| format!("decoding object {id}"))?;
        Ok(Some(object))
    }

    /// Batch-fetch objects by id with a chunked multi-get, returning the decoded objects that were
    /// found (missing ids and per-object errors are silently omitted — used to warm the package
    /// cache, where a miss just falls through to the lazy single fetch). Retries transient errors
    /// per chunk like [`Self::fetch_object`].
    pub async fn fetch_objects(&self, ids: &[ObjectID]) -> Result<Vec<Object>> {
        // Each result carries the full BCS object, and packages can be ~MBs, so keep chunks well
        // under the 128 MiB gRPC message cap.
        const CHUNK: usize = 50;
        let mut out = Vec::new();
        for chunk in ids.chunks(CHUNK) {
            let mut attempt = 0u32;
            let response = loop {
                let mut ledger = self.client.clone().ledger_client();
                let mut request = rpc::BatchGetObjectsRequest::default()
                    .with_read_mask(FieldMask::from_paths(["bcs"]));
                for id in chunk {
                    request
                        .requests_mut()
                        .push(rpc::GetObjectRequest::new(&id.into_bytes().into()));
                }
                match ledger.batch_get_objects(request).await {
                    Ok(response) => break response.into_inner(),
                    Err(status) if attempt < MAX_RETRIES && is_transient(&status) => {
                        let seed = chunk.first().map_or(0, |id| id.into_bytes()[0]);
                        tokio::time::sleep(backoff_delay(attempt, seed)).await;
                        attempt = attempt.saturating_add(1);
                    }
                    Err(status) => {
                        return Err(anyhow::anyhow!(status)).with_context(|| {
                            format!("batch_get_objects({} ids) failed", chunk.len())
                        });
                    }
                }
            };
            for result in response.objects {
                let Ok(object) = result.to_result() else {
                    continue; // per-object NotFound / error: skip, let the lazy path handle it.
                };
                let Some(bytes) = object.bcs.and_then(|b| b.value) else {
                    continue;
                };
                match bcs::from_bytes::<Object>(&bytes) {
                    Ok(object) => out.push(object),
                    Err(e) => warn!("decoding batched object: {e:#}"),
                }
            }
        }
        Ok(out)
    }
}

/// Backoff delay for retry `attempt`: 200ms, 400ms, ... capped at ~5s, plus a little jitter
/// (seeded per-id) to avoid synchronized retries against a rate-limited node.
fn backoff_delay(attempt: u32, seed: u8) -> std::time::Duration {
    let base_ms = 200u64.saturating_mul(1 << attempt.min(5));
    let jitter = (seed as u64) % 100;
    std::time::Duration::from_millis(base_ms.min(5000).saturating_add(jitter))
}

/// Whether a gRPC error is worth retrying — rate limiting (429) and transient unavailability.
fn is_transient(status: &tonic::Status) -> bool {
    use tonic::Code;
    matches!(
        status.code(),
        Code::ResourceExhausted | Code::Unavailable | Code::Aborted | Code::DeadlineExceeded
    ) || status.message().contains("429")
}

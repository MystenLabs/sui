// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Thin gRPC client over a Sui fullnode, used for the two things the checkpoint stream does not
//! already provide: resolving an epoch to its checkpoint range + protocol version, and fetching
//! package objects that are not present in a checkpoint's object set.

use std::future::Future;
use std::str::FromStr as _;
use std::time::Duration;

use anyhow::{Context as _, Result};
use backoff::ExponentialBackoff;
use backoff::future::retry;
use futures::StreamExt as _;
use futures::TryStreamExt as _;
use sui_rpc::field::{FieldMask, FieldMaskUtil};
use sui_rpc::proto::proto_to_timestamp_ms;
use sui_rpc::proto::sui::rpc::v2 as rpc;
use sui_types::base_types::ObjectID;
use sui_types::digests::{ChainIdentifier, CheckpointDigest};
use sui_types::object::Object;
use tracing::warn;
use url::Url;

/// The checkpoint range and protocol parameters for a single epoch.
#[derive(Clone, Copy, Debug)]
pub struct EpochBounds {
    pub first_checkpoint: u64,
    pub last_checkpoint: u64,
    pub protocol_version: u64,
    pub reference_gas_price: u64,
    /// The epoch's start timestamp (ms), i.e. its first checkpoint's `timestamp_ms` — the value the
    /// executor expects for `epoch_timestamp_ms`. Sourced directly from `GetEpoch` so we don't have
    /// to fetch the first checkpoint just to read its timestamp.
    pub epoch_start_timestamp_ms: u64,
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
            "start",
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
        let start = epoch_msg
            .start
            .with_context(|| format!("epoch {epoch} missing start timestamp"))?;
        let epoch_start_timestamp_ms = proto_to_timestamp_ms(start)
            .with_context(|| format!("decoding epoch {epoch} start timestamp"))?;
        Ok(EpochBounds {
            first_checkpoint: epoch_msg
                .first_checkpoint
                .with_context(|| format!("epoch {epoch} missing first_checkpoint"))?,
            last_checkpoint: epoch_msg
                .last_checkpoint
                .with_context(|| format!("epoch {epoch} missing last_checkpoint"))?,
            protocol_version,
            reference_gas_price: epoch_msg.reference_gas_price.unwrap_or(0),
            epoch_start_timestamp_ms,
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

    /// Fetch a package object by id at its latest version. Non-system packages are immutable, so the
    /// latest version is the one that executed against it; system packages (versioned per epoch) are
    /// NOT fetched here — they are resolved version-correctly from the framework snapshot (see
    /// [`crate::context`]). Returns `None` if the fullnode does not have it. Retries transient errors (notably HTTP 429
    /// rate-limiting) with exponential backoff, because a dropped package fetch would otherwise
    /// surface as a spurious execution failure rather than a real result.
    pub async fn fetch_object(&self, id: ObjectID) -> Result<Option<Object>> {
        let response = retry_transient(|| async {
            let mut ledger = self.client.clone().ledger_client();
            let request = rpc::GetObjectRequest::new(&id.into_bytes().into())
                .with_read_mask(FieldMask::from_paths(["bcs"]));
            ledger.get_object(request).await.map(|r| r.into_inner())
        })
        .await
        .with_context(|| format!("get_object({id}) failed"))?;
        let Some(bytes) = response.object.and_then(|o| o.bcs).and_then(|b| b.value) else {
            return Ok(None);
        };
        let object: Object =
            bcs::from_bytes(&bytes).with_context(|| format!("decoding object {id}"))?;
        Ok(Some(object))
    }

    /// Batch-fetch objects by id with a chunked multi-get, returning the decoded objects that were
    /// found (missing ids and per-object errors are silently omitted — used to warm the package
    /// cache, where a miss just falls through to the lazy single fetch). Chunks are fetched
    /// concurrently; each chunk retries transient errors like [`Self::fetch_object`].
    pub async fn fetch_objects(&self, ids: &[ObjectID]) -> Result<Vec<Object>> {
        // Each result carries the full BCS object, and packages can be ~MBs, so keep chunks well
        // under the 128 MiB gRPC message cap.
        const CHUNK: usize = 50;
        // How many chunk requests to keep in flight at once.
        const CONCURRENCY: usize = 8;
        let chunks: Vec<Vec<ObjectID>> = ids.chunks(CHUNK).map(|chunk| chunk.to_vec()).collect();
        let batches: Vec<Vec<Object>> = futures::stream::iter(chunks)
            .map(|chunk| self.fetch_object_chunk(chunk))
            .buffer_unordered(CONCURRENCY)
            .try_collect()
            .await?;
        Ok(batches.into_iter().flatten().collect())
    }

    /// Fetch and decode one chunk of objects for [`Self::fetch_objects`], retrying transient errors.
    /// Takes an owned `chunk` so the returned future borrows only `&self` (not the caller's slice),
    /// which lets the chunk futures be driven concurrently by `buffer_unordered`.
    async fn fetch_object_chunk(&self, chunk: Vec<ObjectID>) -> Result<Vec<Object>> {
        let response = retry_transient(|| async {
            let mut ledger = self.client.clone().ledger_client();
            let mut request = rpc::BatchGetObjectsRequest::default()
                .with_read_mask(FieldMask::from_paths(["bcs"]));
            for id in &chunk {
                request
                    .requests_mut()
                    .push(rpc::GetObjectRequest::new(&id.into_bytes().into()));
            }
            ledger
                .batch_get_objects(request)
                .await
                .map(|r| r.into_inner())
        })
        .await
        .with_context(|| format!("batch_get_objects({} ids) failed", chunk.len()))?;
        let mut out = Vec::new();
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
        Ok(out)
    }
}

/// Exponential backoff (with the crate's built-in jitter) for transient gRPC failures: interval
/// caps at 5s and total elapsed at 60s, so a persistently-unreachable node surfaces an error rather
/// than hanging indefinitely.
fn transient_backoff() -> ExponentialBackoff {
    ExponentialBackoff {
        max_interval: Duration::from_secs(5),
        max_elapsed_time: Some(Duration::from_secs(60)),
        ..Default::default()
    }
}

/// Run a fallible gRPC call under [`transient_backoff`], retrying only transient errors (see
/// [`is_transient`]); any other error is returned immediately.
async fn retry_transient<T, F, Fut>(make_future: F) -> Result<T, tonic::Status>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T, tonic::Status>>,
{
    retry(transient_backoff(), || async {
        make_future().await.map_err(|status| {
            if is_transient(&status) {
                backoff::Error::transient(status)
            } else {
                backoff::Error::permanent(status)
            }
        })
    })
    .await
}

/// Whether a gRPC error is worth retrying — rate limiting (429) and transient unavailability.
fn is_transient(status: &tonic::Status) -> bool {
    use tonic::Code;
    matches!(
        status.code(),
        Code::ResourceExhausted | Code::Unavailable | Code::Aborted | Code::DeadlineExceeded
    ) || status.message().contains("429")
}

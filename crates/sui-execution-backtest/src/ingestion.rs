// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! A remote-object-store ingestion client whose chain identifier is supplied up front instead of
//! derived by fetching genesis.
//!
//! The framework's store client derives the chain id with `checkpoint(0)` — fetching and decoding
//! mainnet genesis. Because every checkpoint fetch `try_join`s the chain-id `OnceCell` and the
//! derivation is slow (and repeatedly retried), this throttles the whole ingestion pipeline well
//! past startup, not just once — measured (mainnet, remote-store source) as a sustained ~4x
//! throughput drop. We already know the chain id cheaply from the fullnode's
//! `GetServiceInfo` (see [`crate::grpc::RpcClient::chain_id`]), so we wrap the store client and
//! return that, never touching genesis. Only the remote-store source has this problem; the gRPC
//! source already uses `GetServiceInfo`, and a local store is fast.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use object_store::ClientOptions;
use object_store::http::HttpBuilder;
use sui_indexer_alt_framework::ingestion::ingestion_client::{
    CheckpointResult, IngestionClient, IngestionClientTrait,
};
use sui_indexer_alt_framework::ingestion::store_client::StoreIngestionClient;
use sui_indexer_alt_framework::metrics::IngestionMetrics;
use sui_types::digests::ChainIdentifier;
use url::Url;

/// Wraps an inner ingestion client, returning a fixed `chain_id` rather than letting the inner
/// client derive it (which, for the remote store, means fetching genesis). Checkpoint fetches pass
/// straight through.
struct FixedChainId {
    inner: Arc<dyn IngestionClientTrait>,
    chain_id: ChainIdentifier,
}

#[async_trait]
impl IngestionClientTrait for FixedChainId {
    async fn chain_id(&self) -> Result<ChainIdentifier> {
        Ok(self.chain_id)
    }

    async fn checkpoint(&self, checkpoint: u64) -> CheckpointResult {
        self.inner.checkpoint(checkpoint).await
    }

    async fn latest_checkpoint_number(&self) -> Result<u64> {
        self.inner.latest_checkpoint_number().await
    }
}

/// Build an [`IngestionClient`] over the remote HTTP object store at `url` whose chain id is fixed
/// to `chain_id` (so it never fetches genesis). Mirrors the framework's own remote-store setup.
pub(crate) fn remote_store_client(
    url: &Url,
    chain_id: ChainIdentifier,
    metrics: Arc<IngestionMetrics>,
) -> Result<IngestionClient> {
    let store = HttpBuilder::new()
        .with_url(url.to_string())
        .with_client_options(ClientOptions::default().with_allow_http(true))
        .build()
        .map(Arc::new)?;
    let inner = Arc::new(StoreIngestionClient::new(
        store,
        Some(metrics.total_ingested_bytes.clone()),
    ));
    let client = Arc::new(FixedChainId { inner, chain_id });
    Ok(IngestionClient::from_trait(client, metrics))
}

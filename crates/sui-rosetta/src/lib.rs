// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::SocketAddr;
use std::num::NonZeroUsize;
use std::string::ToString;
use std::sync::Arc;

use axum::routing::post;
use axum::{Extension, Router};
use lru::LruCache;
use move_core_types::language_storage::TypeTag;
use once_cell::sync::Lazy;
use tokio::sync::Mutex;
use tracing::info;

use crate::grpc_client::GrpcClient;
use sui_sdk::{SuiClient, SUI_COIN_TYPE};

use crate::errors::Error;
use crate::errors::Error::MissingMetadata;
use crate::state::{CheckpointBlockProvider, OnlineServerContext};
use crate::types::{Currency, CurrencyMetadata, SuiEnv};

#[cfg(test)]
#[path = "unit_tests/lib_tests.rs"]
mod lib_tests;

/// This lib implements the Rosetta online and offline server defined by the [Rosetta API Spec](https://www.rosetta-api.org/docs/Reference.html)
mod account;
mod block;
mod construction;
mod errors;
pub mod grpc_client;
mod network;
pub mod operations;
mod state;
pub mod types;

pub static SUI: Lazy<Currency> = Lazy::new(|| Currency {
    symbol: "SUI".to_string(),
    decimals: 9,
    metadata: CurrencyMetadata {
        coin_type: SUI_COIN_TYPE.to_string(),
    },
});

pub struct RosettaOnlineServer {
    env: SuiEnv,
    context: OnlineServerContext,
}

impl RosettaOnlineServer {
    pub fn new(env: SuiEnv, _client: SuiClient, grpc_client: GrpcClient) -> Self {
        let coin_cache = CoinMetadataCache::new(grpc_client.clone(), NonZeroUsize::new(1000).unwrap());
        let blocks = Arc::new(CheckpointBlockProvider::new(
            grpc_client.clone(),
            coin_cache.clone(),
        ));
        Self {
            env,
            context: OnlineServerContext::new(grpc_client, blocks, coin_cache),
        }
    }

    pub async fn serve(self, addr: SocketAddr) {
        // Online endpoints
        let app = Router::new()
            .route("/account/balance", post(account::balance))
            .route("/account/coins", post(account::coins))
            .route("/block", post(block::block))
            .route("/block/transaction", post(block::transaction))
            .route("/construction/submit", post(construction::submit))
            .route("/construction/metadata", post(construction::metadata))
            .route("/network/status", post(network::status))
            .route("/network/list", post(network::list))
            .route("/network/options", post(network::options))
            .layer(Extension(self.env))
            .with_state(self.context);

        let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();

        info!(
            "Sui Rosetta online server listening on {}",
            listener.local_addr().unwrap()
        );
        axum::serve(listener, app).await.unwrap();
    }
}

pub struct RosettaOfflineServer {
    env: SuiEnv,
}

impl RosettaOfflineServer {
    pub fn new(env: SuiEnv) -> Self {
        Self { env }
    }

    pub async fn serve(self, addr: SocketAddr) {
        // Online endpoints
        let app = Router::new()
            .route("/construction/derive", post(construction::derive))
            .route("/construction/payloads", post(construction::payloads))
            .route("/construction/combine", post(construction::combine))
            .route("/construction/preprocess", post(construction::preprocess))
            .route("/construction/hash", post(construction::hash))
            .route("/construction/parse", post(construction::parse))
            .route("/network/list", post(network::list))
            .route("/network/options", post(network::options))
            .layer(Extension(self.env));
        let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();

        info!(
            "Sui Rosetta offline server listening on {}",
            listener.local_addr().unwrap()
        );
        axum::serve(listener, app).await.unwrap();
    }
}

#[derive(Clone)]
pub struct CoinMetadataCache {
    grpc_client: grpc_client::GrpcClient,
    metadata: Arc<Mutex<LruCache<TypeTag, Currency>>>,
}

impl CoinMetadataCache {
    pub fn new(grpc_client: grpc_client::GrpcClient, size: NonZeroUsize) -> Self {
        Self {
            grpc_client,
            metadata: Arc::new(Mutex::new(LruCache::new(size))),
        }
    }

    pub async fn get_currency(&self, type_tag: &TypeTag) -> Result<Currency, Error> {
        let mut cache = self.metadata.lock().await;
        if !cache.contains(type_tag) {
            let coin_info_response = self
                .grpc_client
                .get_coin_info(type_tag.to_string())
                .await?;

            let coin_metadata = coin_info_response
                .metadata
                .ok_or(MissingMetadata)?;

            let ccy = Currency {
                symbol: coin_metadata.symbol.unwrap_or_default(),
                decimals: coin_metadata.decimals.unwrap_or(0) as u64,
                metadata: CurrencyMetadata {
                    coin_type: type_tag.to_string(),
                },
            };
            cache.push(type_tag.clone(), ccy);
        }
        cache.get(type_tag).cloned().ok_or(MissingMetadata)
    }
}

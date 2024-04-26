// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::{routing::get, Router};

pub mod accept;
mod checkpoints;
mod client;
mod error;
mod health;
mod info;
mod objects;
mod response;
pub mod types;

pub use client::Client;
pub use error::{RestError, Result};
pub use sui_types::full_checkpoint_content::{CheckpointData, CheckpointTransaction};
use sui_types::storage::ReadStore;

pub const TEXT_PLAIN_UTF_8: &str = "text/plain; charset=utf-8";
pub const APPLICATION_BCS: &str = "application/bcs";
pub const APPLICATION_JSON: &str = "application/json";

#[derive(Clone)]
pub struct RestService {
    store: std::sync::Arc<dyn ReadStore + Send + Sync>,
    chain_id: sui_types::digests::ChainIdentifier,
    software_version: &'static str,
}

impl RestService {
    pub fn new(
        store: std::sync::Arc<dyn ReadStore + Send + Sync>,
        chain_id: sui_types::digests::ChainIdentifier,
        software_version: &'static str,
    ) -> Self {
        Self {
            store,
            chain_id,
            software_version,
        }
    }

    pub fn new_without_version(
        store: std::sync::Arc<dyn ReadStore + Send + Sync>,
        chain_id: sui_types::digests::ChainIdentifier,
    ) -> Self {
        Self::new(store, chain_id, "unknown")
    }

    pub fn chain_id(&self) -> sui_types::digests::ChainIdentifier {
        self.chain_id
    }

    pub fn software_version(&self) -> &'static str {
        self.software_version
    }

    pub fn into_router(self) -> Router {
        rest_router(self.store.clone())
            .merge(
                Router::new()
                    .route("/", get(info::node_info))
                    .with_state(self.clone()),
            )
            .layer(axum::middleware::map_response_with_state(
                self,
                response::append_info_headers,
            ))
    }

    pub async fn start_service(self, socket_address: std::net::SocketAddr, base: Option<String>) {
        let mut app = self.into_router();

        if let Some(base) = base {
            app = Router::new().nest(&base, app);
        }

        axum::Server::bind(&socket_address)
            .serve(app.into_make_service())
            .await
            .unwrap();
    }
}

fn rest_router<S>(state: S) -> Router
where
    S: ReadStore + Clone + Send + Sync + 'static,
{
    Router::new()
        .route(health::HEALTH_PATH, get(health::health::<S>))
        .route(
            checkpoints::GET_FULL_CHECKPOINT_PATH,
            get(checkpoints::get_full_checkpoint::<S>),
        )
        .route(
            checkpoints::GET_CHECKPOINT_PATH,
            get(checkpoints::get_checkpoint::<S>),
        )
        .route(
            checkpoints::GET_LATEST_CHECKPOINT_PATH,
            get(checkpoints::get_latest_checkpoint::<S>),
        )
        .route(objects::GET_OBJECT_PATH, get(objects::get_object::<S>))
        .route(
            objects::GET_OBJECT_WITH_VERSION_PATH,
            get(objects::get_object_with_version::<S>),
        )
        .with_state(state)
}

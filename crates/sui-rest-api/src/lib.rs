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

pub fn rest_router<S>(state: S) -> Router
where
    S: ReadStore + Clone + Send + Sync + 'static,
{
    let service = RestService {
        store: std::sync::Arc::new(state.clone()),
    };

    Router::new()
        .merge(
            Router::new()
                .route("/", get(info::node_info))
                .with_state(service.clone()),
        )
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
        .layer(axum::middleware::map_response_with_state(
            service,
            response::append_info_headers,
        ))
        .with_state(state)
}

pub async fn start_service<S>(socket_address: std::net::SocketAddr, state: S, base: Option<String>)
where
    S: ReadStore + Clone + Send + Sync + 'static,
{
    let app = if let Some(base) = base {
        Router::new().nest(&base, rest_router(state))
    } else {
        rest_router(state)
    };

    axum::Server::bind(&socket_address)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

#[derive(Clone)]
struct RestService {
    store: std::sync::Arc<dyn ReadStore + Send + Sync>,
    //TODO
    // add git revision
    // node_type
    // chain_id
}

impl RestService {
    pub fn chain_id(&self) -> sui_types::digests::ChainIdentifier {
        //TODO FIX THIS
        sui_types::messages_checkpoint::CheckpointDigest::new([0; 32]).into()
    }
    pub fn git_revision(&self) -> std::borrow::Cow<'static, str> {
        //TODO populate this
        "unknown git-rev".into()
    }

    pub fn node_type(&self) -> info::NodeType {
        //TODO populate this
        info::NodeType::Fullnode
    }
}

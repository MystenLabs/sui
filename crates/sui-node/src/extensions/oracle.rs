use anyhow::bail;
use axum::{extract::State, routing::get, Json, Router};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
};
use tower_http::cors::{Any, CorsLayer};

use move_core_types::account_address::AccountAddress;

use sui_exex::{ExExContext, ExExEvent, ExExNotification};
use sui_types::{
    base_types::ObjectID,
    id::UID,
    object::{Data, Object},
};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PublisherStorage {
    id: UID,
    price: u128,
    name: String,
}

/// Main Oracle function
pub async fn exex_oracle(mut ctx: ExExContext) -> anyhow::Result<()> {
    tracing::info!("[node-{}] 🧩 Oracle ExEx initiated!", ctx.identifier);

    // Initialize shared state
    let app_state = Arc::new(AppState::default());
    let api = Api::new(app_state.clone(), SocketAddr::from(([127, 0, 0, 1], 8080)));

    // Start the API server in a separate task
    tokio::spawn(async move {
        api.start().await;
    });

    // Main loop to process notifications
    while let Some(notification) = ctx.notifications.next().await {
        let checkpoint_number = match notification {
            ExExNotification::CheckpointSynced { checkpoint_number } => {
                tracing::info!(
                    "[node-{}] 🤖 Oracle updating at checkpoint #{} !",
                    ctx.identifier,
                    checkpoint_number,
                );
                checkpoint_number
            }
        };

        let object = ctx
            .object_store
            .get_object(&ObjectID::from_address(
                AccountAddress::from_hex(
                    "1d71061f46e99efecd8d7ee1a77f331cff3afa892aa48b2ab85938dfc6d12b33",
                )
                .unwrap(),
            ))
            .unwrap()
            .unwrap();

        let storage: PublisherStorage = deserialize_object(&object)?;

        {
            // Update shared state with the latest price and name
            let mut latest_price = app_state.latest_price.lock().unwrap();
            *latest_price = Some(storage.clone());
        }

        ctx.events
            .send(ExExEvent::FinishedHeight(checkpoint_number))?;
    }

    Ok(())
}

fn deserialize_object<'a, T: Deserialize<'a>>(object: &'a Object) -> anyhow::Result<T> {
    match &object.as_inner().data {
        Data::Move(o) => Ok(bcs::from_bytes(o.contents())?),
        Data::Package(_) => bail!("Object should not be a Package"),
    }
}

#[derive(Debug, Clone, Default)]
pub struct AppState {
    latest_price: Arc<Mutex<Option<PublisherStorage>>>,
}

pub struct Api {
    state: Arc<AppState>,
    address: SocketAddr,
}

impl Api {
    /// Creates a new API instance
    pub fn new(state: Arc<AppState>, address: SocketAddr) -> Self {
        Self { state, address }
    }

    /// Starts the HTTP server
    pub async fn start(&self) {
        let app = Router::new()
            .route("/price", get(Self::get_price))
            .layer(CorsLayer::new().allow_origin(Any))
            .with_state(self.state.clone());

        tracing::info!("🌐 HTTP API starting at {}", self.address);
        let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
        axum::serve(listener, app).await.unwrap();
    }

    /// Handler for the /price endpoint
    async fn get_price(State(state): State<Arc<AppState>>) -> Json<Option<PublisherStorage>> {
        let latest_price = state.latest_price.lock().unwrap();
        Json(latest_price.clone())
    }
}

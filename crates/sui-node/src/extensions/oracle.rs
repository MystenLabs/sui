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
    base_types::{ObjectID, SuiAddress},
    collection_types::{Table, TableVec},
    id::UID,
    object::Data,
    storage::ObjectStore,
};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PublisherStorage {
    id: UID,
    publisher_name: String,
    price: Option<u128>,
    timestamp: Option<u64>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Publisher {
    id: UID,
    name: String,
    address: SuiAddress,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Registry {
    id: UID,
    owner: SuiAddress,
    publishers: TableVec,
    publishers_storages: Table,
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

    let _registry: Registry = deserialize_object(
        &ctx.object_store,
        "397c1042ea417d457357e5d61047e72e741133bd99c88ba775a4be35895d138e",
    )?;
    dbg!(&_registry);

    // Call dynamic_fields on all publishers_storages->fields->id

    // Gives an object_id, read it, gives object with name;value.
    // name = key / value = value.
    // key = wallet of the publisher // value = storage id.

    // store all storage id that will get checked at each checkpoints.

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

        let storage: PublisherStorage = deserialize_object(
            &ctx.object_store,
            "be8ff73ec47b158a5ce884cada70912ca4b28a7cdf013f4d180c1298137f487a",
        )?;

        {
            let mut latest_price = app_state.latest_price.lock().unwrap();
            *latest_price = Some(storage.clone());
        }

        ctx.events
            .send(ExExEvent::FinishedHeight(checkpoint_number))?;
    }

    Ok(())
}

fn deserialize_object<'a, T: Deserialize<'a>>(
    object_store: &Arc<dyn ObjectStore + Send + Sync>,
    address: &str,
) -> anyhow::Result<T> {
    let object = object_store
        .get_object(&ObjectID::from_address(
            AccountAddress::from_hex(address).unwrap(),
        ))
        .unwrap()
        .unwrap();

    match object.as_inner().data.clone() {
        Data::Move(o) => {
            let boxed_contents = Box::leak(o.contents().to_vec().into_boxed_slice());
            Ok(bcs::from_bytes(boxed_contents)?)
        }
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

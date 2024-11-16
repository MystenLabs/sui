use anyhow::{bail, Context};
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
    collection_types::{VecMap, VecSet},
    id::{ID, UID},
    object::Data,
    storage::ObjectStore,
};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PuiPriceStorage {
    id: UID,
    publisher_name: String,
    price: Option<u128>,
    timestamp: Option<u64>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PuiPublisher {
    name: String,
    address: SuiAddress,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PuiRegistry {
    id: UID,
    owner: SuiAddress,
    publishers: VecSet<PuiPublisher>,
    publishers_storages: VecMap<SuiAddress, ID>,
}

const REGISTRY_ID: &str = "9862bbb25c7e28708b08a6107633e34258c842f480117538fdfac177b69088af";

/// Main Oracle function
pub async fn exex_oracle(mut ctx: ExExContext) -> anyhow::Result<()> {
    let registry_id =
        AccountAddress::from_hex(REGISTRY_ID).context("Serializing the Account Address")?;
    let oracle_registry: PuiRegistry = deserialize_object(&ctx.object_store, registry_id)
        .context("Fetching the Oracle PuiRegistry")?;

    let app_state = Arc::new(AppState::default());
    let api = Api::new(app_state.clone(), SocketAddr::from(([127, 0, 0, 1], 8080)));
    tokio::spawn(async move {
        api.start().await;
    });

    tracing::info!("[node-{}] 🧩 Oracle ExEx initiated!", ctx.identifier);
    while let Some(notification) = ctx.notifications.next().await {
        let started_at = std::time::Instant::now();
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

        let mut storages: Vec<PuiPriceStorage> = vec![];
        for entry in oracle_registry.publishers_storages.contents.iter() {
            let storage_id = &entry.value;
            let storage: PuiPriceStorage = deserialize_object(
                &ctx.object_store,
                AccountAddress::from_bytes(storage_id.bytes)?,
            )
            .context("Fetching a Price Storage")?;
            storages.push(storage);
        }

        let aggregated_price = calculate_aggregated_price(&storages);
        {
            let mut price_data = app_state.price_data.lock().unwrap();
            *price_data = aggregated_price;
        }

        tracing::info!(
            "[node-{}] Executed {} in {:?}",
            ctx.identifier,
            checkpoint_number,
            started_at.elapsed()
        );
        ctx.events
            .send(ExExEvent::FinishedHeight(checkpoint_number))?;
    }

    Ok(())
}

fn calculate_aggregated_price(storages: &[PuiPriceStorage]) -> AggregatedPrice {
    let mut prices: Vec<u128> = storages
        .iter()
        .filter_map(|storage| storage.price)
        .collect();

    let latest_timestamp = storages
        .iter()
        .filter_map(|storage| storage.timestamp)
        .max();

    let median_price = if !prices.is_empty() {
        prices.sort_unstable();
        let mid = prices.len() / 2;
        if prices.len() % 2 == 0 {
            Some((prices[mid - 1] + prices[mid]) / 2)
        } else {
            Some(prices[mid])
        }
    } else {
        None
    };

    AggregatedPrice {
        median_price,
        latest_timestamp,
        storage_count: storages.len(),
    }
}

fn deserialize_object<'a, T: Deserialize<'a>>(
    object_store: &Arc<dyn ObjectStore + Send + Sync>,
    address: AccountAddress,
) -> anyhow::Result<T> {
    let object = object_store
        .get_object(&ObjectID::from_address(address))?
        .context("Fetching the object")?;

    match object.as_inner().data.clone() {
        Data::Move(o) => {
            let boxed_contents = Box::leak(o.contents().to_vec().into_boxed_slice());
            Ok(bcs::from_bytes(boxed_contents)?)
        }
        Data::Package(_) => bail!("Object should not be a Package"),
    }
}

#[derive(Default, Serialize, Deserialize, Debug, Clone)]
pub struct AggregatedPrice {
    median_price: Option<u128>,
    latest_timestamp: Option<u64>,
    storage_count: usize,
}

#[derive(Debug, Clone, Default)]
pub struct AppState {
    price_data: Arc<Mutex<AggregatedPrice>>,
}

pub struct Api {
    state: Arc<AppState>,
    address: SocketAddr,
}

impl Api {
    pub fn new(state: Arc<AppState>, address: SocketAddr) -> Self {
        Self { state, address }
    }

    pub async fn start(&self) {
        let app = Router::new()
            .route("/price", get(Self::get_price))
            .layer(CorsLayer::new().allow_origin(Any))
            .with_state(self.state.clone());

        tracing::info!("🌐 HTTP API starting at {}", self.address);
        let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
        axum::serve(listener, app).await.unwrap();
    }

    async fn get_price(State(state): State<Arc<AppState>>) -> Json<AggregatedPrice> {
        let price_data = state.price_data.lock().unwrap();
        Json(price_data.clone())
    }
}

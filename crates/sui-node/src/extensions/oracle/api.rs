use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{Arc, Mutex},
};

use axum::{extract::State, routing::get, Json, Router};
use serde::{Deserialize, Serialize};
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use tokio::sync::mpsc::Receiver;
use tower_http::cors::{Any, CorsLayer};

use super::SignedData;

#[derive(Default, Serialize, Deserialize, Debug, Clone)]
pub struct MedianPrice {
    pub pair: String,
    pub median_price: Option<u128>,
    pub timestamp: Option<u64>,
    pub checkpoint: Option<CheckpointSequenceNumber>,
}

const DEFAULT_API_PORT: u16 = 3000;

#[derive(Debug, Clone, Default)]
pub struct AppState {
    pub price_data: Arc<Mutex<(MedianPrice, Vec<SignedData<MedianPrice>>)>>,
}

#[derive(Debug)]
pub struct Api {
    state: Arc<AppState>,
    address: SocketAddr,
    quorum_rx: Receiver<(MedianPrice, Vec<SignedData<MedianPrice>>)>,
}

impl Api {
    pub fn new(
        host: [u8; 4],
        quorum_rx: Receiver<(MedianPrice, Vec<SignedData<MedianPrice>>)>,
    ) -> Self {
        let api_port = std::env::var("API_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(DEFAULT_API_PORT);
        Self {
            state: Arc::new(AppState::default()),
            address: SocketAddr::new(IpAddr::V4(Ipv4Addr::from(host)), api_port),
            quorum_rx,
        }
    }

    pub async fn start(self) {
        let address = self.address;
        let state = self.state.clone();
        tokio::spawn(async move {
            Self::expose_api(state.clone(), address).await;
        });

        let mut quorum_rx = self.quorum_rx;
        let state = self.state.clone();
        tokio::spawn(async move {
            while let Some(consensus_price) = quorum_rx.recv().await {
                Self::update_exposed_price(state.clone(), consensus_price).await;
            }
        });
    }

    async fn expose_api(state: Arc<AppState>, address: SocketAddr) {
        let app = Router::new()
            .route("/price", get(Self::get_price))
            .layer(CorsLayer::new().allow_origin(Any))
            .with_state(state.clone());

        tracing::info!("[Oracle ExEx] 🌐 HTTP API starting at {}", address);
        let listener = tokio::net::TcpListener::bind(address).await.unwrap();
        axum::serve(listener, app).await.unwrap();
    }

    async fn get_price(
        State(state): State<Arc<AppState>>,
    ) -> Json<(MedianPrice, Vec<SignedData<MedianPrice>>)> {
        let price_data = state.price_data.lock().unwrap();
        Json(price_data.clone())
    }

    /// Updates the exposed price in the API.
    async fn update_exposed_price(
        state: Arc<AppState>,
        consensus_price: (MedianPrice, Vec<SignedData<MedianPrice>>),
    ) {
        let mut price_data = state.price_data.lock().expect("Poisoned lock");
        *price_data = consensus_price;
    }
}

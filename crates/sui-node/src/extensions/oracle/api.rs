use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use axum::{extract::State, routing::get, Json, Router};
use serde::{Deserialize, Serialize};
use tower_http::cors::{Any, CorsLayer};

#[derive(Default, Serialize, Deserialize, Debug, Clone)]
pub struct AggregatedPrice {
    pub pair: String,
    pub median_price: Option<u128>,
    pub timestamp: Option<u64>,
}

#[derive(Debug, Clone, Default)]
pub struct AppState {
    pub price_data: Arc<Mutex<AggregatedPrice>>,
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

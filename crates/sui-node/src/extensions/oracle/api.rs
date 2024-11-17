use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use axum::{extract::State, routing::get, Json, Router};
use serde::{Deserialize, Serialize};
use tower_http::cors::{Any, CorsLayer};

#[derive(Default, Serialize, Deserialize, Debug, Clone)]
pub struct MedianPrice {
    pub pair: String,
    pub median_price: Option<u128>,
    pub timestamp: Option<u64>,
}

#[derive(Debug, Clone, Default)]
pub struct AppState {
    pub price_data: Arc<Mutex<MedianPrice>>,
}

#[derive(Clone)]
pub struct Api {
    state: Arc<AppState>,
    address: SocketAddr,
}

impl Api {
    pub fn new(address: SocketAddr) -> Self {
        Self {
            state: Arc::new(AppState::default()),
            address,
        }
    }

    pub async fn start_and_get_state(&self) -> Arc<AppState> {
        let s = self.clone();
        let state = self.state.clone();
        tokio::spawn(async move {
            s.run_forever().await;
        });
        state
    }

    async fn run_forever(&self) {
        let app = Router::new()
            .route("/price", get(Self::get_price))
            .layer(CorsLayer::new().allow_origin(Any))
            .with_state(self.state.clone());

        tracing::info!("🌐 HTTP API starting at {}", self.address);
        let port = std::env::var("API_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(3000);
        let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}"))
            .await
            .unwrap();
        axum::serve(listener, app).await.unwrap();
    }

    async fn get_price(State(state): State<Arc<AppState>>) -> Json<MedianPrice> {
        let price_data = state.price_data.lock().unwrap();
        Json(price_data.clone())
    }
}

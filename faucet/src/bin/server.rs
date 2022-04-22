use axum::{
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Extension, Json, Router,
};
use std::net::SocketAddr;
use sui_faucet::{FaucetRequest, Service, SimpleFaucet};

#[tokio::main]
async fn main() {
    // initialize tracing
    tracing_subscriber::fmt::init();

    let app = Router::new()
        .route("/", get(health))
        .route("/gas", post(request_gas))
        .layer(Extension(Service::new(SimpleFaucet::default())));

    let addr = SocketAddr::from(([127, 0, 0, 1], 5003));
    tracing::debug!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

/// basic handler that responds with a static string
async fn health() -> &'static str {
    "OK"
}

/// handler for all the request_gas requests
async fn request_gas(
    Json(payload): Json<FaucetRequest>,
    Extension(svc): Extension<Service>,
) -> impl IntoResponse {
    let resp = svc.execute(payload).await;
    (StatusCode::CREATED, Json(resp))
}

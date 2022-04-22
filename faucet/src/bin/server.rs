use axum::{
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Extension, Json, Router,
};
use std::net::SocketAddr;
use sui::{
    sui_config_dir,
    wallet_commands::{WalletCommands, WalletContext},
    SUI_WALLET_CONFIG,
};
use sui_faucet::{FaucetRequest, Service, SimpleFaucet};
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // initialize tracing
    tracing_subscriber::fmt::init();

    let context = create_wallet_context().await?;

    let app = Router::new()
        .route("/", get(health))
        .route("/gas", post(request_gas))
        .layer(Extension(Service::new(SimpleFaucet::new(context))));

    let addr = SocketAddr::from(([127, 0, 0, 1], 5003));
    info!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
    Ok(())
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

async fn create_wallet_context() -> Result<WalletContext, anyhow::Error> {
    // Create Wallet context.
    // TODO: Make the path configurable
    let wallet_conf = sui_config_dir()?.join(SUI_WALLET_CONFIG);
    info!("Initialize wallet from config path: {:?}", wallet_conf);
    let mut context = WalletContext::new(&wallet_conf)?;
    let address = context.config.accounts.first().cloned().unwrap();

    // Sync client to retrieve objects from the network.
    WalletCommands::SyncClientState {
        address: Some(address),
    }
    .execute(&mut context)
    .await
    .map_err(|err| anyhow::anyhow!("Fail to sync client state: {}", err))?;
    Ok(context)
}

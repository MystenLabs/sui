use anyhow::Result;
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use clap::{Parser, Subcommand};
// use rand::rngs::OsRng;
use simulacrum::{AdvanceEpochConfig, InMemoryStore, Simulacrum};
use std::{
    net::SocketAddr,
    sync::{Arc, RwLock},
};
use sui_swarm_config::{genesis_config::AccountConfig, network_config_builder::ConfigBuilder};
use tower_http::cors::CorsLayer;
use tracing::info;

#[derive(Parser, Debug)]
#[clap(name = "sui-forking")]
#[clap(about = "Minimal CLI for Sui forking with simulacrum", long_about = None)]
struct Args {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Start the forking server
    Start {
        #[clap(long, default_value = "8123")]
        port: u16,

        #[clap(long, default_value = "127.0.0.1")]
        host: String,

        #[clap(long)]
        checkpoint: Option<u64>,

        #[clap(long, default_value = "mainnet")]
        network: String,
    },
    /// Advance checkpoint by 1
    AdvanceCheckpoint {
        #[clap(long, default_value = "http://127.0.0.1:8123")]
        server_url: String,
    },
    /// Advance clock by specified duration in seconds
    AdvanceClock {
        #[clap(long, default_value = "http://127.0.0.1:8123")]
        server_url: String,

        #[clap(long, default_value = "1")]
        seconds: u64,
    },
    /// Advance to next epoch
    AdvanceEpoch {
        #[clap(long, default_value = "http://127.0.0.1:8123")]
        server_url: String,
    },
    /// Get current status
    Status {
        #[clap(long, default_value = "http://127.0.0.1:8123")]
        server_url: String,
    },
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct AdvanceCheckpointRequest;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct AdvanceClockRequest {
    seconds: u64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct AdvanceEpochRequest;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct ApiResponse<T> {
    success: bool,
    data: Option<T>,
    error: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct ForkingStatus {
    checkpoint: u64,
    epoch: u64,
    transaction_count: usize,
}

// type SharedSimulacrum = Arc<RwLock<Simulacrum<OsRng, InMemoryStore>>>;

struct AppState {
    simulacrum: Arc<RwLock<Simulacrum>>,
}

impl AppState {
    fn new() -> Self {
        // Create a network config with a temporary directory
        let simulacrum = Simulacrum::new();
        // let mut rng = OsRng;
        // let config = ConfigBuilder::new_with_temp_dir()
        //     .rng(&mut rng)
        //     .with_chain_start_timestamp_ms(1)
        //     .deterministic_committee_size(std::num::NonZeroUsize::new(1).unwrap())
        //     .build();
        //
        // let store = InMemoryStore::default();
        // let simulacrum = Simulacrum::new_with_network_config_store(&config, OsRng, store);
        //
        Self {
            simulacrum: Arc::new(RwLock::new(simulacrum)),
        }
    }
}

async fn health() -> &'static str {
    "OK"
}

async fn get_status(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let sim = state.simulacrum.read().unwrap();
    let store = sim.store();

    let checkpoint = store
        .get_highest_checkpint()
        .map(|c| c.sequence_number)
        .unwrap_or(0);

    // Get the current epoch from the checkpoint
    let epoch = store
        .get_highest_checkpint()
        .map(|c| c.epoch())
        .unwrap_or(0);

    let status = ForkingStatus {
        checkpoint,
        epoch,
        transaction_count: 0, // TODO: get actual transaction count
    };

    Json(ApiResponse {
        success: true,
        data: Some(status),
        error: None,
    })
}

async fn advance_checkpoint(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let mut sim = state.simulacrum.write().unwrap();

    // create_checkpoint returns a VerifiedCheckpoint, not a Result
    let checkpoint = sim.create_checkpoint();
    info!("Advanced to checkpoint {}", checkpoint.sequence_number);

    Json(ApiResponse::<String> {
        success: true,
        data: Some(format!(
            "Advanced to checkpoint {}",
            checkpoint.sequence_number
        )),
        error: None,
    })
}

async fn advance_clock(
    State(state): State<Arc<AppState>>,
    Json(request): Json<AdvanceClockRequest>,
) -> impl IntoResponse {
    let mut sim = state.simulacrum.write().unwrap();

    let duration = std::time::Duration::from_secs(request.seconds);
    sim.advance_clock(duration);
    info!("Advanced clock by {} seconds", request.seconds);

    Json(ApiResponse::<String> {
        success: true,
        data: Some(format!("Clock advanced by {} seconds", request.seconds)),
        error: None,
    })
}

async fn advance_epoch(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let mut sim = state.simulacrum.write().unwrap();

    // Use default configuration for advancing epoch
    let config = AdvanceEpochConfig::default();
    sim.advance_epoch(config);
    info!("Advanced to next epoch");

    Json(ApiResponse::<String> {
        success: true,
        data: Some("Advanced to next epoch".to_string()),
        error: None,
    })
}

async fn start_server(host: String, port: u16) -> Result<()> {
    let state = Arc::new(AppState::new());

    let app = Router::new()
        .route("/health", get(health))
        .route("/status", get(get_status))
        .route("/advance-checkpoint", post(advance_checkpoint))
        .route("/advance-clock", post(advance_clock))
        .route("/advance-epoch", post(advance_epoch))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", host, port).parse()?;
    info!("Starting forking server on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn send_command(url: &str, endpoint: &str, body: Option<serde_json::Value>) -> Result<()> {
    let client = reqwest::Client::new();
    let full_url = format!("{}/{}", url, endpoint);

    let response = if let Some(body) = body {
        client.post(&full_url).json(&body).send().await?
    } else {
        client.post(&full_url).send().await?
    };

    if response.status().is_success() {
        let result: ApiResponse<serde_json::Value> = response.json().await?;
        if result.success {
            println!("Success: {:?}", result.data);
        } else {
            println!("Error: {:?}", result.error);
        }
    } else {
        println!("Server error: {}", response.status());
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    match args.command {
        Commands::Start {
            host,
            port,
            checkpoint,
            network,
        } => {
            info!(
                "Starting forking server for {} at checkpoint {:?}",
                network, checkpoint
            );
            start_server(host, port).await?
        }
        Commands::AdvanceCheckpoint { server_url } => {
            send_command(&server_url, "advance-checkpoint", None).await?
        }
        Commands::AdvanceClock {
            server_url,
            seconds,
        } => {
            let body = serde_json::json!(AdvanceClockRequest { seconds });
            send_command(&server_url, "advance-clock", Some(body)).await?
        }
        Commands::AdvanceEpoch { server_url } => {
            send_command(&server_url, "advance-epoch", None).await?
        }
        Commands::Status { server_url } => {
            let client = reqwest::Client::new();
            let response = client.get(format!("{}/status", server_url)).send().await?;

            if response.status().is_success() {
                let result: ApiResponse<ForkingStatus> = response.json().await?;
                if let Some(status) = result.data {
                    println!("Checkpoint: {}", status.checkpoint);
                    println!("Epoch: {}", status.epoch);
                    println!("Transactions: {}", status.transaction_count);
                } else {
                    println!("Error: {:?}", result.error);
                }
            } else {
                println!("Server error: {}", response.status());
            }
        }
    }

    Ok(())
}

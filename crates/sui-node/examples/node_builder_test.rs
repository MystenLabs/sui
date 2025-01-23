use std::path::PathBuf;
use sui_node::builder::NodeBuilder;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging with debug level
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::DEBUG)
        .with_file(true)
        .with_line_number(true)
        .with_thread_ids(true)
        .with_target(true)
        .with_thread_names(true)
        .with_ansi(true)
        .init();

    info!("Starting node builder test...");
    info!("Loading config from crates/sui-node/examples/node_config.yaml");

    // Create a new node builder
    let builder =
        NodeBuilder::new().with_config(PathBuf::from("crates/sui-node/examples/node_config.yaml"));

    info!("Starting node...");
    // Run the node
    let (_handle, _runtimes) = builder.launch().await?;
    info!("Node started successfully!");
    info!("Press Ctrl+C to shut down");

    // Wait for Ctrl+C
    tokio::signal::ctrl_c().await?;
    info!("Received shutdown signal");
    info!("Shutting down...");

    Ok(())
}

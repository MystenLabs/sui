use std::path::PathBuf;
use sui_exex::ExExContext;
use sui_node::builder::NodeBuilder;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Example extension function that just logs a message
    async fn example_extension(ctx: ExExContext) -> anyhow::Result<()> {
        info!("Extension running...");
        Ok(())
    }

    info!("Starting node builder test...");

    // Create a new node builder
    let builder = NodeBuilder::new()
        .with_config(PathBuf::from("crates/sui-node/examples/node_config.yaml"))
        .with_exex(example_extension);

    // Run the node
    builder.run().await?;

    Ok(())
}

pub mod aggregation;
pub mod api;
pub mod p2p;
pub mod sui;

use aggregation::*;
use api::*;
use p2p::*;
use sui::*;

use anyhow::Context;
use futures::StreamExt;

use move_core_types::account_address::AccountAddress;

use sui_exex::{ExExContext, ExExEvent, ExExNotification};
use sui_types::base_types::ObjectID;

const REGISTRY_ID: &str = "9862bbb25c7e28708b08a6107633e34258c842f480117538fdfac177b69088af";

/// Main loop of the Oracle.
pub async fn exex_oracle(mut ctx: ExExContext) -> anyhow::Result<()> {
    let storage_ids = setup_storage(&ctx)?;

    let (p2p_node, consensus_rx) = setup_p2p().await?;
    Api::new([127, 0, 0, 1], consensus_rx).start().await;

    tracing::info!("[node-{}] 🧩 Oracle ExEx initiated!", ctx.identifier);
    while let Some(notification) = ctx.notifications.next().await {
        let checkpoint = match notification {
            ExExNotification::CheckpointSynced { checkpoint_number } => checkpoint_number,
        };
        tracing::info!(
            "[node-{}] 🤖 Oracle updating at checkpoint #{} !",
            ctx.identifier,
            checkpoint,
        );
        fetch_and_broadcast_median(&ctx, &p2p_node, &storage_ids, checkpoint).await?;
        ctx.events.send(ExExEvent::FinishedHeight(checkpoint))?;
    }

    p2p_node.shutdown().await?;
    Ok(())
}

/// Retrieves all the registered Price Storages from the Oracle Registry.
fn setup_storage(ctx: &ExExContext) -> anyhow::Result<Vec<ObjectID>> {
    let registry_id =
        AccountAddress::from_hex(REGISTRY_ID).context("Serializing the Account Address")?;

    let oracle_registry: PuiRegistry = deserialize_object(&ctx.object_store, registry_id)
        .context("Fetching the Oracle PuiRegistry")?;

    let storage_ids = oracle_registry
        .publishers_storages
        .contents
        .iter()
        .map(|entry| ObjectID::from_address(AccountAddress::from_bytes(entry.value.bytes).unwrap()))
        .collect();

    Ok(storage_ids)
}

/// Fetch from the storages the published prices, computes the median & sends
/// it to the P2P channel.
async fn fetch_and_broadcast_median(
    ctx: &ExExContext,
    p2p_node: &P2PNodeHandle,
    storage_ids: &[ObjectID],
    checkpoint: u64,
) -> anyhow::Result<()> {
    let started_at = std::time::Instant::now();

    let price_storages: Vec<PuiPriceStorage> = deserialize_objects(&ctx.object_store, storage_ids)?;
    let median_price = aggregate_to_median(&price_storages);
    let _ = p2p_node.broadcast_price(median_price, checkpoint).await;
    tracing::info!(
        "[node-{}] ✅ Executed {} in {:?}",
        ctx.identifier,
        checkpoint,
        started_at.elapsed()
    );

    Ok(())
}

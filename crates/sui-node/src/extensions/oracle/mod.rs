pub mod aggregation;
pub mod api;
pub mod p2p;
pub mod sui_objects;

use aggregation::*;
use api::*;
use p2p::*;
use sui_objects::*;

use anyhow::Context;
use futures::StreamExt;

use move_core_types::account_address::AccountAddress;

use sui_exex::{ExExContext, ExExEvent, ExExNotification};
use sui_types::base_types::ObjectID;

/// The Registry ID where we registered the publishers of the Oracle.
const REGISTRY_ID: &str = "c1f6d875d562097b58bae7eb8341aa59428b7b793d1b3b4fe34b8dce0c82dbf6";

/// Main loop of the Oracle.
pub async fn exex_oracle(mut ctx: ExExContext) -> anyhow::Result<()> {
    let (p2p_broadcaster, consensus_rx) = start_p2p().await?;
    Api::new([127, 0, 0, 1], consensus_rx).start().await;

    tracing::info!("🧩 Oracle ExEx initiated!");
    while let Some(notification) = ctx.notifications.next().await {
        let checkpoint = match notification {
            ExExNotification::CheckpointSynced { checkpoint_number } => checkpoint_number,
        };
        let storage_ids = match setup_storage(&ctx) {
            Ok(s) => s,
            Err(_) => {
                ctx.events.send(ExExEvent::FinishedHeight(checkpoint))?;
                continue;
            }
        };
        tracing::info!("🤖 Oracle updating at checkpoint #{} !", checkpoint,);
        let started_at = std::time::Instant::now();
        if let Some(median_price) = fetch_prices_and_aggregate(&ctx, &storage_ids).await? {
            let _ = p2p_broadcaster.broadcast(median_price, checkpoint).await;
        }
        tracing::info!("✅ Executed {} in {:?}", checkpoint, started_at.elapsed());
        ctx.events.send(ExExEvent::FinishedHeight(checkpoint))?;
    }

    Ok(())
}

/// Retrieves all the registered Price Storages from the Oracle Registry.
fn setup_storage(ctx: &ExExContext) -> anyhow::Result<Vec<ObjectID>> {
    let registry_id =
        AccountAddress::from_hex(REGISTRY_ID).context("Serializing the Account Address")?;

    let oracle_registry: PuiRegistry =
        deserialize_object(&ctx.store, registry_id).context("Fetching the Oracle PuiRegistry")?;

    let storage_ids = oracle_registry
        .publishers_storages
        .contents
        .iter()
        .map(|entry| ObjectID::from_address(AccountAddress::from_bytes(entry.value.bytes).unwrap()))
        .collect();

    Ok(storage_ids)
}

/// Fetch from the storages the published prices and computes the median.
/// The median can be None if no prices are published.
async fn fetch_prices_and_aggregate(
    ctx: &ExExContext,
    storage_ids: &[ObjectID],
) -> anyhow::Result<Option<MedianPrice>> {
    let price_storages: Vec<PuiPriceStorage> = deserialize_objects(&ctx.store, storage_ids)?;
    Ok(aggregate_to_median(&price_storages))
}

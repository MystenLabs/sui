pub mod aggregation;
pub mod api;
pub mod p2p;
pub mod sui;

use aggregation::*;
use api::*;
use p2p::*;
use sui::*;

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Context;
use futures::StreamExt;

use move_core_types::account_address::AccountAddress;

use sui_exex::{ExExContext, ExExEvent, ExExNotification};
use sui_types::base_types::ObjectID;

const REGISTRY_ID: &str = "9862bbb25c7e28708b08a6107633e34258c842f480117538fdfac177b69088af";

/// Main Oracle function
pub async fn exex_oracle(mut ctx: ExExContext) -> anyhow::Result<()> {
    let storage_ids = setup_storage(&ctx)?;
    let (p2p_node, mut consensus_rx) = setup_p2p().await?;
    let app_state = Api::new(SocketAddr::from(([127, 0, 0, 1], 8080)))
        .start_and_get_state()
        .await;

    tracing::info!("[node-{}] 🧩 Oracle ExEx initiated!", ctx.identifier);

    tokio::spawn(async move {
        while let Some(consensus_price) = consensus_rx.recv().await {
            update_consensus_price(&app_state, consensus_price).await;
        }
    });

    // Handle notifications in the main task
    loop {
        tokio::select! {
            Some(notification) = ctx.notifications.next() => {
                let checkpoint_number = match notification {
                    ExExNotification::CheckpointSynced { checkpoint_number } => {
                        tracing::info!(
                            "[node-{}] 🤖 Oracle updating at checkpoint #{} !",
                            ctx.identifier,
                            checkpoint_number,
                        );
                        checkpoint_number
                    }
                };

                if let Err(e) = handle_new_checkpoint(&ctx, &p2p_node, &storage_ids, checkpoint_number).await {
                    tracing::error!("Error handling checkpoint: {}", e);
                    break;
                }

                if let Err(e) = ctx.events.send(ExExEvent::FinishedHeight(checkpoint_number)) {
                    tracing::error!("Error sending finished height event: {}", e);
                    break;
                }
            }

            else => break,
        }
    }

    // Clean shutdown
    p2p_node.shutdown().await?;
    Ok(())
}

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

async fn handle_new_checkpoint(
    ctx: &ExExContext,
    p2p_node: &P2PNodeHandle,
    storage_ids: &[ObjectID],
    checkpoint_number: u64,
) -> anyhow::Result<()> {
    let started_at = std::time::Instant::now();

    let price_storages: Vec<PuiPriceStorage> = deserialize_objects(&ctx.object_store, storage_ids)?;
    let median_price = aggregate_to_median(&price_storages);

    if let Err(e) = p2p_node
        .broadcast_price(median_price, checkpoint_number)
        .await
    {
        tracing::error!("Failed to broadcast price: {}", e);
    } else {
        tracing::info!(
            "Price broadcasted to P2P network for checkpoint {}",
            checkpoint_number
        );
    }

    tracing::info!(
        "[node-{}] ✅ Executed {} in {:?}",
        ctx.identifier,
        checkpoint_number,
        started_at.elapsed()
    );

    Ok(())
}

async fn update_consensus_price(app_state: &Arc<AppState>, consensus_price: MedianPrice) {
    let mut price_data = app_state.price_data.lock().expect("Poisoned lock");
    *price_data = consensus_price;
}

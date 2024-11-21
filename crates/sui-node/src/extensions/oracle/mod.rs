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

/// Main loop of the Oracle with improved state management and error handling
pub async fn exex_oracle(mut ctx: ExExContext) -> anyhow::Result<()> {
    tracing::info!("🧩 Oracle ExEx initiated!");
    tracing::info!("⏳ Syncing ExEx to blockchain tip...");

    let mut state = ExExOracleState::Syncing;
    while let Some(notification) = ctx.notifications.next().await {
        let checkpoint = match notification {
            ExExNotification::CheckpointSynced { checkpoint_number } => checkpoint_number,
        };

        // Check if we've reached the tip
        if matches!(state, ExExOracleState::Syncing) {
            if let Some(chain_tip) = ctx.highest_known_checkpoint_sequence_number() {
                if chain_tip == checkpoint {
                    tracing::info!("🥳 ExEx reached tip! Starting P2P and API services...");
                    state = state.transition_to_operating().await?;
                } else {
                    ctx.events.send(ExExEvent::FinishedHeight(checkpoint))?;
                    continue;
                }
            } else {
                ctx.events.send(ExExEvent::FinishedHeight(checkpoint))?;
                continue;
            }
        }

        // When the tip has been reached, we can process checkpoints.
        tracing::info!("🤖 Oracle updating at checkpoint #{checkpoint}!");
        let started_at = std::time::Instant::now();
        process_checkpoint(&ctx, &state, checkpoint).await?;
        tracing::info!("✅ Executed {checkpoint} in {:?}", started_at.elapsed());
        ctx.events.send(ExExEvent::FinishedHeight(checkpoint))?;
    }

    Ok(())
}

/// Fetches the price from the publishers storage and broadcast the price to
/// the P2P network.
async fn process_checkpoint(
    ctx: &ExExContext,
    state: &ExExOracleState,
    checkpoint: u64,
) -> anyhow::Result<()> {
    let storage_ids = setup_storage(ctx);
    if storage_ids.is_err() {
        tracing::warn!("😱 Storage setup failed for checkpoint {checkpoint}. Ignoring.");
        return Ok(());
    }

    if let Some(median_price) = fetch_prices_and_aggregate(ctx, &storage_ids?).await? {
        state.broadcast_price(median_price, checkpoint).await?;
    }
    Ok(())
}

/// Retrieves all the registered Price Storages from the Oracle Registry.
fn setup_storage(ctx: &ExExContext) -> anyhow::Result<Vec<ObjectID>> {
    let registry_id =
        AccountAddress::from_hex(REGISTRY_ID).context("Serializing the Account Address")?;

    let oracle_registry: PuiRegistry =
        deserialize_object(&ctx.store, registry_id).context("Fetching the Oracle PuiRegistry")?;

    oracle_registry
        .publishers_storages
        .contents
        .iter()
        .map(|entry| {
            AccountAddress::from_bytes(entry.value.bytes)
                .map(ObjectID::from_address)
                .context("Invalid storage address")
        })
        .collect()
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

#[derive(Debug)]
enum ExExOracleState {
    Syncing,
    Operating { p2p_broadcaster: P2PBroadcaster },
}

impl ExExOracleState {
    async fn transition_to_operating(self) -> anyhow::Result<Self> {
        match self {
            ExExOracleState::Syncing => {
                let (p2p_broadcaster, consensus_rx) = start_p2p().await?;
                let api = Api::new([127, 0, 0, 1], consensus_rx);

                tokio::spawn(async move {
                    api.start().await;
                });

                Ok(ExExOracleState::Operating { p2p_broadcaster })
            }
            state @ ExExOracleState::Operating { .. } => Ok(state),
        }
    }

    async fn broadcast_price(&self, price: MedianPrice, checkpoint: u64) -> anyhow::Result<()> {
        match self {
            ExExOracleState::Operating { p2p_broadcaster } => {
                p2p_broadcaster.broadcast(price, checkpoint).await?;
                Ok(())
            }
            ExExOracleState::Syncing => Err(anyhow::anyhow!("Cannot broadcast while syncing")),
        }
    }
}

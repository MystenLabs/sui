pub mod aggregation;
pub mod api;
pub mod p2p;
pub mod sui_objects;

use aggregation::*;
use api::*;
use p2p::*;
use sui_objects::*;

use futures::StreamExt;
use move_core_types::account_address::AccountAddress;
use sui_exex::{ExExContext, ExExEvent, ExExNotification};
use sui_types::{base_types::ObjectID, messages_checkpoint::CheckpointSequenceNumber};

const REGISTRY_ID: &str = "c1f6d875d562097b58bae7eb8341aa59428b7b793d1b3b4fe34b8dce0c82dbf6";

pub async fn exex_oracle(mut ctx: ExExContext) -> anyhow::Result<()> {
    tracing::info!("🧩 Oracle ExEx initiated!");
    tracing::info!("⏳ Syncing ExEx to blockchain tip...");

    let mut oracle_state: Option<ExExOracleState> = None;

    while let Some(ExExNotification::CheckpointSynced { checkpoint_number }) =
        ctx.notifications.next().await
    {
        // Initialize oracle_state when we reach the chain tip
        if oracle_state.is_none() {
            if let Some(chain_tip) = ctx.highest_known_checkpoint_sequence_number() {
                if chain_tip == checkpoint_number {
                    tracing::info!(
                        "🥳 ExEx reached tip #{}! Starting P2P and API services...",
                        chain_tip
                    );
                    oracle_state = Some(ExExOracleState::initialize().await?);
                }
            }
        }

        // Process checkpoint if we're synced
        if let Some(ref oracle_state) = oracle_state {
            if let Err(e) = process_checkpoint(&ctx, oracle_state, checkpoint_number).await {
                tracing::error!(
                    error = %e,
                    checkpoint = %checkpoint_number,
                    "Failed to process checkpoint"
                );
            }
        }

        ctx.events
            .send(ExExEvent::FinishedHeight(checkpoint_number))?;
    }

    Ok(())
}

async fn process_checkpoint(
    ctx: &ExExContext,
    oracle_state: &ExExOracleState,
    checkpoint: CheckpointSequenceNumber,
) -> anyhow::Result<()> {
    let publishers_storages = fetch_publishers_storages(ctx).map_err(|e| {
        tracing::warn!(
            error = %e,
            checkpoint = %checkpoint,
            "😱 Storage setup failed. Skipping checkpoint."
        );
        e
    })?;

    if let Some(median_price) = aggregate_to_median(publishers_storages, checkpoint) {
        oracle_state
            .broadcast_price(median_price, checkpoint)
            .await?;
    }

    Ok(())
}

fn fetch_publishers_storages(ctx: &ExExContext) -> anyhow::Result<Vec<PuiPriceStorage>> {
    let registry_id = AccountAddress::from_hex(REGISTRY_ID)?;
    let oracle_registry: PuiRegistry = deserialize_object(&ctx.store, registry_id)?;

    let storage_ids: Vec<ObjectID> = oracle_registry
        .publishers_storages
        .contents
        .iter()
        .try_fold::<_, _, anyhow::Result<_>>(
        Vec::with_capacity(oracle_registry.publishers.contents.len()),
        |mut acc, entry| {
            let object_id = AccountAddress::from_bytes(entry.value.bytes)
                .map(ObjectID::from_address)
                .map_err(|_| anyhow::anyhow!("Invalid storage"))?;
            acc.push(object_id);
            Ok(acc)
        },
    )?;

    deserialize_objects(&ctx.store, &storage_ids)
}

#[derive(Debug)]
pub struct ExExOracleState {
    p2p_broadcaster: P2PBroadcaster,
}

impl ExExOracleState {
    async fn initialize() -> anyhow::Result<Self> {
        let (p2p_broadcaster, consensus_rx) = start_p2p().await?;
        let api = Api::new([127, 0, 0, 1], consensus_rx);

        tokio::spawn(async move {
            api.start().await;
        });

        Ok(ExExOracleState { p2p_broadcaster })
    }

    async fn broadcast_price(
        &self,
        price: MedianPrice,
        checkpoint: CheckpointSequenceNumber,
    ) -> anyhow::Result<()> {
        self.p2p_broadcaster.broadcast(price, checkpoint).await
    }
}

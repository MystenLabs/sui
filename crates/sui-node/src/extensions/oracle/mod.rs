pub mod aggregation;
pub mod api;
pub mod sui;

use aggregation::*;
use api::*;
use sui::*;

use std::{net::SocketAddr, sync::Arc};

use anyhow::Context;
use futures::StreamExt;

use move_core_types::account_address::AccountAddress;

use sui_exex::{ExExContext, ExExEvent, ExExNotification};
use sui_types::base_types::ObjectID;

const REGISTRY_ID: &str = "9862bbb25c7e28708b08a6107633e34258c842f480117538fdfac177b69088af";

/// Main Oracle function
pub async fn exex_oracle(mut ctx: ExExContext) -> anyhow::Result<()> {
    let registry_id =
        AccountAddress::from_hex(REGISTRY_ID).context("Serializing the Account Address")?;
    let oracle_registry: PuiRegistry = deserialize_object(&ctx.object_store, registry_id)
        .context("Fetching the Oracle PuiRegistry")?;
    let storage_ids: Vec<ObjectID> = oracle_registry
        .publishers_storages
        .contents
        .iter()
        .map(|entry| ObjectID::from_address(AccountAddress::from_bytes(entry.value.bytes).unwrap()))
        .collect();

    let app_state = Arc::new(AppState::default());
    let api = Api::new(app_state.clone(), SocketAddr::from(([127, 0, 0, 1], 8080)));
    tokio::spawn(async move {
        api.start().await;
    });

    tracing::info!("[node-{}] 🧩 Oracle ExEx initiated!", ctx.identifier);
    while let Some(notification) = ctx.notifications.next().await {
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

        let started_at = std::time::Instant::now();
        let storages: Vec<PuiPriceStorage> = deserialize_objects(&ctx.object_store, &storage_ids)?;
        let aggregated_price = calculate_aggregated_price(&storages);

        let mut price_data = app_state.price_data.lock().expect("Poisoned lock");
        *price_data = aggregated_price;
        drop(price_data);

        tracing::info!(
            "[node-{}] ✅ Executed {} in {:?}",
            ctx.identifier,
            checkpoint_number,
            started_at.elapsed()
        );
        ctx.events
            .send(ExExEvent::FinishedHeight(checkpoint_number))?;
    }

    Ok(())
}

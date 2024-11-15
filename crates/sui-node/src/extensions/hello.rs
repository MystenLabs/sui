use futures::StreamExt;

use sui_exex::{ExExContext, ExExEvent, ExExNotification};

pub async fn exex_hello(mut ctx: ExExContext) -> anyhow::Result<()> {
    tracing::info!("[node-{}] 🧩 Hello ExEx initiated!", ctx.identifier);
    while let Some(notification) = ctx.notifications.next().await {
        let id = match notification {
            ExExNotification::CheckpointSynced { checkpoint_number } => {
                tracing::info!(
                    "[node-{}] 👋 Hello Checkpoint #{} !",
                    ctx.identifier,
                    checkpoint_number,
                );
                checkpoint_number
            }
            ExExNotification::EpochTerminated { epoch_id } => {
                tracing::info!("[node-{}] 👋🥳 Hello Epoch #{} !", ctx.identifier, epoch_id);
                epoch_id
            }
        };

        // TODO: We should make the dinstinction between FinishedHeight and FinishedEpoch?
        ctx.events.send(ExExEvent::FinishedHeight(id))?;
    }
    Ok(())
}

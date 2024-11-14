use futures::StreamExt;

use sui_exex::{ExExContext, ExExEvent, ExExNotification};

pub async fn exex_hello(mut ctx: ExExContext) -> anyhow::Result<()> {
    tracing::info!("🧩 Created the Hello ExEx!");
    while let Some(notification) = ctx.notifications.next().await {
        let id = match notification {
            ExExNotification::CheckpointSynced { checkpoint } => {
                tracing::info!(
                    "[node-{}] 👋 Hello Checkpoint #{} !",
                    ctx.identifier,
                    checkpoint,
                );
                checkpoint
            }
            ExExNotification::EpochTerminated { epoch } => {
                tracing::info!("[node-{}] 👋🥳 Hello Epoch #{} !", ctx.identifier, epoch);
                epoch
            }
        };

        // TODO: This is bad. We should make the dinstinction between FinishedHeight and FinishedEpoch.
        ctx.events.send(ExExEvent::FinishedHeight(id))?;
    }
    Ok(())
}

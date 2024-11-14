use futures::StreamExt;

use sui_exex::{ExExContext, ExExEvent, ExExNotification};

pub async fn exex_hello(mut ctx: ExExContext) -> anyhow::Result<()> {
    tracing::info!("🧩 Created the Hello ExEx!");
    while let Some(notification) = ctx.notifications.next().await {
        let checkpoint = match notification {
            ExExNotification::CheckpointSynced { checkpoint } => checkpoint,
        };

        tracing::info!("👋 Hello checkpoint #{} !", checkpoint);
        ctx.events
            .send(ExExEvent::FinishedHeight(checkpoint))?;
    }
    Ok(())
}

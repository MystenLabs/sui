use std::{
    pin::Pin,
    task::{Context, Poll},
};
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

use futures::Stream;
use tokio::sync::mpsc::Receiver;

/// Notifications sent to an `ExEx`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ExExNotification {
    /// A new checkpoint got synced by the full node.
    CheckpointSynced {
        checkpoint_number: CheckpointSequenceNumber,
    },
}

/// A stream of [`ExExNotification`]s. The stream will emit notifications for all blocks.
#[derive(Debug)]
pub struct ExExNotifications {
    notifications: Receiver<ExExNotification>,
}

impl ExExNotifications {
    /// Creates a new instance of [`ExExNotifications`].
    pub const fn new(notifications: Receiver<ExExNotification>) -> Self {
        Self { notifications }
    }
}

impl Stream for ExExNotifications {
    type Item = ExExNotification;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.get_mut().notifications.poll_recv(cx)
    }
}

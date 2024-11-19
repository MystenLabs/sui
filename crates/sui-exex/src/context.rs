use std::sync::Arc;

use mysten_metrics::monitored_mpsc::UnboundedSender;
use sui_types::storage::{ObjectStore, ReadStore, WriteStore};

use crate::{notification::ExExNotifications, ExExEvent};

pub trait ExExStore: ObjectStore + WriteStore + ReadStore + Send + Sync {}

/// Captures the context that an `ExEx` has access to.
pub struct ExExContext {
    pub store: Arc<dyn ExExStore>,
    /// Channel used to send [`ExExEvent`]s to the rest of the node.
    ///
    /// # Important
    ///
    /// The exex should emit a `FinishedHeight` whenever a processed block is safe to prune.
    /// Additionally, the exex can pre-emptively emit a `FinishedHeight` event to specify what
    /// blocks to receive notifications for.
    pub events: UnboundedSender<ExExEvent>,

    /// Channel to receive [`ExExNotification`]s.
    ///
    /// # Important
    ///
    /// Once an [`ExExNotification`] is sent over the channel, it is
    /// considered delivered by the node.
    pub notifications: ExExNotifications,
}

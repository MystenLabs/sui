use std::sync::Arc;

use mysten_metrics::monitored_mpsc::UnboundedSender;
use sui_types::{digests::ChainIdentifier, storage::ObjectStore};

use crate::{notification::ExExNotifications, ExExEvent};

/// Captures the context that an `ExEx` has access to.
pub struct ExExContext {
    /// Full-node unique identifier
    pub identifier: ChainIdentifier,

    /// TODO: "head" equivalent? In reth, points to the head of the chain:
    /// https://github.com/paradigmxyz/reth/blob/main/crates/ethereum-forks/src/head.rs#L14

    /// TODO: "config" -> the full node configuration used
    pub object_store: Arc<dyn ObjectStore + Send + Sync>,

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

use std::sync::Arc;

use mysten_metrics::monitored_mpsc::UnboundedSender;
use sui_network::state_sync;
use sui_types::{
    messages_checkpoint::CheckpointSequenceNumber,
    storage::{ObjectStore, ReadStore, WriteStore},
};

use crate::{notification::ExExNotifications, ExExEvent};

// We can't directly import the `RocksDbStore` object from `sui-core` so we create this trait
// to describe the ExEx Storage instead.
pub trait ExExStore: ObjectStore + WriteStore + ReadStore + Send + Sync {}

/// Captures the context that an `ExEx` has access to.
pub struct ExExContext {
    pub store: Arc<dyn ExExStore>,

    /// Handle to the StateSync subsystem.
    /// Used to retrieve the highest known checkpoint number using the `PeerHeights`
    /// struct and the `highest_known_checkpoint_sequence_number` function.
    pub state_sync_handle: state_sync::Handle,

    /// Channel used to send `ExExEvent`s to the rest of the node.
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

impl ExExContext {
    /// Returns the highest checkpoint known by the network.
    /// Corresponds to the tip of the chain - checkpoints wise.
    pub fn highest_known_checkpoint_sequence_number(&self) -> Option<CheckpointSequenceNumber> {
        self.state_sync_handle
            .highest_known_checkpoint_sequence_number()
    }
}

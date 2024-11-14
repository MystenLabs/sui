use sui_types::messages_checkpoint::CheckpointSequenceNumber;

/// Events emitted by an `ExEx`.
pub enum ExExEvent {
    FinishedHeight(CheckpointSequenceNumber),
}

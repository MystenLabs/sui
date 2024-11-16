use sui_types::messages_checkpoint::CheckpointSequenceNumber;

/// Events emitted by an `ExEx`.
#[derive(Debug)]
pub enum ExExEvent {
    /// TODO: This should probably be renamed. Is "Height" the correct terminology in Sui's context?
    /// Also, check the pruning used in Reth. Maybe this is not needed for us.
    FinishedHeight(CheckpointSequenceNumber),
}

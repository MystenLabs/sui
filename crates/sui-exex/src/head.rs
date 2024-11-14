use sui_types::messages_checkpoint::CheckpointSequenceNumber;

/// A head of the ExEx. It contains the highest host block committed to the
/// internal ExEx state. I.e. the latest block that the ExEx has fully
/// processed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExExHead {
    /// The head block.
    pub checkpoint: CheckpointSequenceNumber,
}

/// The finished height of all `ExEx`'s.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinishedExExHeight {
    /// No `ExEx`'s are installed, so there is no finished height.
    NoExExs,
    /// Not all `ExExs` have emitted a `FinishedHeight` event yet.
    NotReady,
    /// The finished height of all `ExEx`'s.
    ///
    /// This is the lowest common denominator between all `ExEx`'s.
    ///
    /// This block is used to (amongst other things) determine what blocks are safe to prune.
    ///
    /// The number is inclusive, i.e. all blocks `<= finished_height` are safe to prune.
    Height(CheckpointSequenceNumber),
}

impl FinishedExExHeight {
    /// Returns `true` if not all `ExExs` have emitted a `FinishedHeight` event yet.
    pub const fn is_not_ready(&self) -> bool {
        matches!(self, Self::NotReady)
    }
}

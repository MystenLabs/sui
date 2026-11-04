// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use super::error::Result;
use crate::base_types::VerifiedExecutionData;
use crate::committee::Committee;
use crate::messages_checkpoint::{
    CheckpointContents, VerifiedCheckpoint, VerifiedCheckpointContents,
};
use crate::storage::ReadStore;

pub trait WriteStore: ReadStore {
    fn insert_checkpoint(&self, checkpoint: &VerifiedCheckpoint) -> Result<()>;
    fn update_highest_synced_checkpoint(&self, checkpoint: &VerifiedCheckpoint) -> Result<()>;
    fn update_highest_verified_checkpoint(&self, checkpoint: &VerifiedCheckpoint) -> Result<()>;
    fn insert_checkpoint_contents(
        &self,
        checkpoint: &VerifiedCheckpoint,
        contents: VerifiedCheckpointContents,
    ) -> Result<()>;

    /// Inserts verified transactions and effects of a checkpoint whose contents are synced in
    /// pages, without the checkpoint contents.
    fn insert_transactions_and_effects(&self, data: &[VerifiedExecutionData]) -> Result<()>;

    /// Records the contents of a checkpoint without its full contents. The caller must have
    /// inserted every transaction and effects of the checkpoint first, because readers treat
    /// stored contents as proof that they are available.
    fn insert_checkpoint_contents_digests(
        &self,
        checkpoint: &VerifiedCheckpoint,
        contents: CheckpointContents,
    ) -> Result<()>;

    fn insert_committee(&self, new_committee: Committee) -> Result<()>;
}

impl<T: WriteStore + ?Sized> WriteStore for &T {
    fn insert_checkpoint(&self, checkpoint: &VerifiedCheckpoint) -> Result<()> {
        (*self).insert_checkpoint(checkpoint)
    }

    fn update_highest_synced_checkpoint(&self, checkpoint: &VerifiedCheckpoint) -> Result<()> {
        (*self).update_highest_synced_checkpoint(checkpoint)
    }

    fn update_highest_verified_checkpoint(&self, checkpoint: &VerifiedCheckpoint) -> Result<()> {
        (*self).update_highest_verified_checkpoint(checkpoint)
    }

    fn insert_checkpoint_contents(
        &self,
        checkpoint: &VerifiedCheckpoint,
        contents: VerifiedCheckpointContents,
    ) -> Result<()> {
        (*self).insert_checkpoint_contents(checkpoint, contents)
    }

    fn insert_transactions_and_effects(&self, data: &[VerifiedExecutionData]) -> Result<()> {
        (*self).insert_transactions_and_effects(data)
    }

    fn insert_checkpoint_contents_digests(
        &self,
        checkpoint: &VerifiedCheckpoint,
        contents: CheckpointContents,
    ) -> Result<()> {
        (*self).insert_checkpoint_contents_digests(checkpoint, contents)
    }

    fn insert_committee(&self, new_committee: Committee) -> Result<()> {
        (*self).insert_committee(new_committee)
    }
}

impl<T: WriteStore + ?Sized> WriteStore for Box<T> {
    fn insert_checkpoint(&self, checkpoint: &VerifiedCheckpoint) -> Result<()> {
        (**self).insert_checkpoint(checkpoint)
    }

    fn update_highest_synced_checkpoint(&self, checkpoint: &VerifiedCheckpoint) -> Result<()> {
        (**self).update_highest_synced_checkpoint(checkpoint)
    }

    fn update_highest_verified_checkpoint(&self, checkpoint: &VerifiedCheckpoint) -> Result<()> {
        (**self).update_highest_verified_checkpoint(checkpoint)
    }

    fn insert_checkpoint_contents(
        &self,
        checkpoint: &VerifiedCheckpoint,
        contents: VerifiedCheckpointContents,
    ) -> Result<()> {
        (**self).insert_checkpoint_contents(checkpoint, contents)
    }

    fn insert_transactions_and_effects(&self, data: &[VerifiedExecutionData]) -> Result<()> {
        (**self).insert_transactions_and_effects(data)
    }

    fn insert_checkpoint_contents_digests(
        &self,
        checkpoint: &VerifiedCheckpoint,
        contents: CheckpointContents,
    ) -> Result<()> {
        (**self).insert_checkpoint_contents_digests(checkpoint, contents)
    }

    fn insert_committee(&self, new_committee: Committee) -> Result<()> {
        (**self).insert_committee(new_committee)
    }
}

impl<T: WriteStore + ?Sized> WriteStore for Arc<T> {
    fn insert_checkpoint(&self, checkpoint: &VerifiedCheckpoint) -> Result<()> {
        (**self).insert_checkpoint(checkpoint)
    }

    fn update_highest_synced_checkpoint(&self, checkpoint: &VerifiedCheckpoint) -> Result<()> {
        (**self).update_highest_synced_checkpoint(checkpoint)
    }

    fn update_highest_verified_checkpoint(&self, checkpoint: &VerifiedCheckpoint) -> Result<()> {
        (**self).update_highest_verified_checkpoint(checkpoint)
    }

    fn insert_checkpoint_contents(
        &self,
        checkpoint: &VerifiedCheckpoint,
        contents: VerifiedCheckpointContents,
    ) -> Result<()> {
        (**self).insert_checkpoint_contents(checkpoint, contents)
    }

    fn insert_transactions_and_effects(&self, data: &[VerifiedExecutionData]) -> Result<()> {
        (**self).insert_transactions_and_effects(data)
    }

    fn insert_checkpoint_contents_digests(
        &self,
        checkpoint: &VerifiedCheckpoint,
        contents: CheckpointContents,
    ) -> Result<()> {
        (**self).insert_checkpoint_contents_digests(checkpoint, contents)
    }

    fn insert_committee(&self, new_committee: Committee) -> Result<()> {
        (**self).insert_committee(new_committee)
    }
}

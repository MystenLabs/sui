// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use super::error::Result;
use crate::committee::Committee;
use crate::messages_checkpoint::{VerifiedCheckpoint, VerifiedCheckpointContents};
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

    fn insert_committee(&self, new_committee: Committee) -> Result<()> {
        (**self).insert_committee(new_committee)
    }
}

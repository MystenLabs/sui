// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::committee::Committee;
use crate::messages_checkpoint::{VerifiedCheckpoint, VerifiedCheckpointContents};
use crate::storage::ReadStore;

pub trait WriteStore: ReadStore {
    fn insert_checkpoint(&self, checkpoint: &VerifiedCheckpoint) -> Result<(), Self::Error>;
    fn update_highest_synced_checkpoint(
        &self,
        checkpoint: &VerifiedCheckpoint,
    ) -> Result<(), Self::Error>;
    fn update_highest_verified_checkpoint(
        &self,
        checkpoint: &VerifiedCheckpoint,
    ) -> Result<(), Self::Error>;
    fn insert_checkpoint_contents(
        &self,
        checkpoint: &VerifiedCheckpoint,
        contents: VerifiedCheckpointContents,
    ) -> Result<(), Self::Error>;

    fn insert_committee(&self, new_committee: Committee) -> Result<(), Self::Error>;
}

impl<T: WriteStore> WriteStore for &T {
    fn insert_checkpoint(&self, checkpoint: &VerifiedCheckpoint) -> Result<(), Self::Error> {
        WriteStore::insert_checkpoint(*self, checkpoint)
    }

    fn update_highest_synced_checkpoint(
        &self,
        checkpoint: &VerifiedCheckpoint,
    ) -> Result<(), Self::Error> {
        WriteStore::update_highest_synced_checkpoint(*self, checkpoint)
    }

    fn update_highest_verified_checkpoint(
        &self,
        checkpoint: &VerifiedCheckpoint,
    ) -> Result<(), Self::Error> {
        WriteStore::update_highest_verified_checkpoint(*self, checkpoint)
    }

    fn insert_checkpoint_contents(
        &self,
        checkpoint: &VerifiedCheckpoint,
        contents: VerifiedCheckpointContents,
    ) -> Result<(), Self::Error> {
        WriteStore::insert_checkpoint_contents(*self, checkpoint, contents)
    }

    fn insert_committee(&self, new_committee: Committee) -> Result<(), Self::Error> {
        WriteStore::insert_committee(*self, new_committee)
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    base_types::{ObjectID, VersionNumber},
    committee::EpochId,
};
use std::sync::Arc;

pub trait ConfigStore {
    /// Return the initial sequence number in the epoch for the given object_id if present in the
    /// epoch marker table. Otherwise returns the current version of the object.
    fn get_current_epoch_stable_sequence_number(
        &self,
        object_id: &ObjectID,
        epoch_id: EpochId,
    ) -> Option<VersionNumber>;
}

impl<T: ConfigStore + ?Sized> ConfigStore for &T {
    fn get_current_epoch_stable_sequence_number(
        &self,
        object_id: &ObjectID,
        epoch_id: EpochId,
    ) -> Option<VersionNumber> {
        (*self).get_current_epoch_stable_sequence_number(object_id, epoch_id)
    }
}

impl<T: ConfigStore + ?Sized> ConfigStore for Box<T> {
    fn get_current_epoch_stable_sequence_number(
        &self,
        object_id: &ObjectID,
        epoch_id: EpochId,
    ) -> Option<VersionNumber> {
        (**self).get_current_epoch_stable_sequence_number(object_id, epoch_id)
    }
}

impl<T: ConfigStore + ?Sized> ConfigStore for Arc<T> {
    fn get_current_epoch_stable_sequence_number(
        &self,
        object_id: &ObjectID,
        epoch_id: EpochId,
    ) -> Option<VersionNumber> {
        (**self).get_current_epoch_stable_sequence_number(object_id, epoch_id)
    }
}

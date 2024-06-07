// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use sui_sdk2::types::{EpochId, ValidatorCommittee};
use sui_sdk2::types::{Object, ObjectId, Version};
use sui_types::storage::error::Result;
use sui_types::storage::ObjectStore;
use sui_types::storage::RestStateReader;

#[derive(Clone)]
pub struct StateReader {
    inner: Arc<dyn RestStateReader>,
}

impl StateReader {
    pub fn new(inner: Arc<dyn RestStateReader>) -> Self {
        Self { inner }
    }

    pub fn inner(&self) -> &Arc<dyn RestStateReader> {
        &self.inner
    }

    #[allow(unused)]
    pub fn get_object(&self, object_id: ObjectId) -> Result<Option<Object>> {
        self.inner
            .get_object(&object_id.into())
            .map(|maybe| maybe.map(Into::into))
    }

    #[allow(unused)]
    pub fn get_object_with_version(
        &self,
        object_id: ObjectId,
        version: Version,
    ) -> Result<Option<Object>> {
        self.inner
            .get_object_by_key(&object_id.into(), version.into())
            .map(|maybe| maybe.map(Into::into))
    }

    pub fn get_committee(&self, epoch: EpochId) -> Result<Option<ValidatorCommittee>> {
        self.inner
            .get_committee(epoch)
            .map(|maybe| maybe.map(|committee| (*committee).clone().into()))
    }
}

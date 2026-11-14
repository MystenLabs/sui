// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `()` → `()`.
//!
//! Singleton row recording that the startup build of
//! [`super::packages_by_checkpoint`] from [`super::package_versions`] has
//! completed. The row's presence is the record; its value is empty.
//!
//! The build writes this row in the same batch as its last chunk, so a
//! crash mid-build leaves no row and the next open finishes the build.
//! `RpcStoreSchema::open` skips the build while the row exists: after the
//! build, every writer of a `package_versions` row with a known publish
//! checkpoint writes its `packages_by_checkpoint` row in the same batch,
//! so rescanning would find nothing to write.
//!
//! The row lives in its own CF, rather than under a made-up pipeline key
//! in the framework's `__restore` or `__watermark` CFs, because code
//! walks every `__watermark` entry (the pruner's epoch floor) and would
//! read such an entry as a pipeline.

use sui_consistent_store::error::Error;
use sui_consistent_store::reader::Reader;

use crate::schema::primitives::UnitKey;

pub const NAME: &str = "packages_by_checkpoint_built";

pub type Key = UnitKey;
pub type Value = UnitKey;

pub fn options(resolver: &sui_consistent_store::CfOptionsResolver) -> rocksdb::Options {
    resolver.options(NAME)
}

impl<R: Reader> super::RpcStoreSchema<R> {
    /// Whether the startup build of `packages_by_checkpoint` has
    /// completed on this database.
    pub fn packages_by_checkpoint_built(&self) -> Result<bool, Error> {
        Ok(self.packages_by_checkpoint_built.get(&UnitKey)?.is_some())
    }
}

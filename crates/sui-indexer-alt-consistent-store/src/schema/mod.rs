// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use sui_indexer_alt_framework::types::base_types::VersionDigest;

use crate::{
    db::{map::DbMap, Db},
    store,
};

pub(crate) mod object_by_owner;

/// All tables written to and read from the consistent store.
pub(crate) struct Schema {
    /// Fetch objects by their owner, optionally filtered by type. Coin-like objects are returned
    /// in descending balance order.
    pub(crate) object_by_owner: DbMap<object_by_owner::Key, VersionDigest>,
}

impl store::Schema for Schema {
    fn cfs() -> Vec<(&'static str, rocksdb::Options)> {
        vec![("object_by_owner", object_by_owner::options())]
    }

    fn open(db: &Arc<Db>) -> anyhow::Result<Self> {
        Ok(Self {
            object_by_owner: DbMap::new(db.clone(), "object_by_owner"),
        })
    }
}

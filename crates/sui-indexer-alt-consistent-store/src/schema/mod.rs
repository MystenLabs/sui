// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use sui_indexer_alt_framework::types::base_types::VersionDigest;

use crate::{
    db::{Db, map::DbMap},
    store,
};

pub(crate) mod balances;
pub(crate) mod object_by_owner;
pub(crate) mod object_by_type;

/// All tables written to and read from the consistent store.
pub(crate) struct Schema {
    /// The balances of all coin-like objects owned by an account, indexed by owner and type.
    pub(crate) balances: DbMap<balances::Key, i128>,

    /// Fetch objects by their owner, optionally filtered by type. Coin-like objects are returned
    /// in descending balance order.
    pub(crate) object_by_owner: DbMap<object_by_owner::Key, VersionDigest>,

    /// Fetch objects by their type.
    pub(crate) object_by_type: DbMap<object_by_type::Key, VersionDigest>,
}

impl store::Schema for Schema {
    fn cfs(base_options: &rocksdb::Options) -> Vec<(&'static str, rocksdb::Options)> {
        vec![
            ("balances", balances::options(base_options)),
            ("object_by_owner", object_by_owner::options(base_options)),
            ("object_by_type", object_by_type::options(base_options)),
        ]
    }

    fn open(db: &Arc<Db>) -> anyhow::Result<Self> {
        Ok(Self {
            balances: DbMap::new(db.clone(), "balances"),
            object_by_owner: DbMap::new(db.clone(), "object_by_owner"),
            object_by_type: DbMap::new(db.clone(), "object_by_type"),
        })
    }
}

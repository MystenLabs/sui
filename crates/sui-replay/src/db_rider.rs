// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use sui_core::authority::authority_store_tables::{
    AuthorityPerpetualTables, AuthorityPerpetualTablesReadOnly,
};
use sui_storage::indexes::IndexStoreTablesReadOnly;
use sui_storage::IndexStoreTables;
use typed_store::rocks::MetricConf;

pub struct _DBRider {
    pub index_store: IndexStoreTablesReadOnly,
    pub perpatual_store: AuthorityPerpetualTablesReadOnly,
}

impl _DBRider {
    pub fn _open(path: PathBuf) -> Self {
        let mut index_path = path.clone();
        index_path.push("indexes");

        let index_store =
            IndexStoreTables::get_read_only_handle(index_path, None, None, MetricConf::default());
        let mut perpetual_path = path;
        perpetual_path.push("store");
        perpetual_path.push("perpetual");

        let perpatual_store = AuthorityPerpetualTables::get_read_only_handle(
            perpetual_path,
            None,
            None,
            MetricConf::default(),
        );
        Self {
            index_store,
            perpatual_store,
        }
    }
}

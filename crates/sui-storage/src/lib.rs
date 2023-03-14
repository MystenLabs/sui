// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod indexes;
pub use indexes::{IndexStore, IndexStoreTables};

pub mod mutex_table;
pub mod object_store;
pub mod write_ahead_log;
pub mod write_path_pending_tx_log;

use typed_store::rocks::{
    default_db_options as default_rocksdb_options,
    point_lookup_db_options as point_lookup_rocksdb_options, DBOptions,
};

/// Returns default options to use for RocksDB
pub fn default_db_options() -> DBOptions {
    default_rocksdb_options()
}

// Returns default options to use for RocksDB, optimized for point lookup.
pub fn point_lookup_db_options() -> DBOptions {
    point_lookup_rocksdb_options()
}

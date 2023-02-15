// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod indexes;
pub use indexes::{IndexStore, IndexStoreTables};

pub mod event_store;
pub mod mutex_table;
pub mod object_store;
pub mod write_ahead_log;
pub mod write_path_pending_tx_log;

use rocksdb::Options;
use typed_store::rocks::{
    default_db_options as default_rocksdb_options, DBOptions, ReadWriteOptions,
};

/// Given a provided `db_options`, add a few default options.
/// Returns the default option and the point lookup option.
pub fn default_db_options(
    db_options: Option<Options>,
    cache_capacity: Option<usize>,
) -> (DBOptions, DBOptions) {
    let mut db_options = db_options
        .map(|o| DBOptions {
            options: o,
            rw_options: ReadWriteOptions::default(),
        })
        .unwrap_or_else(default_rocksdb_options);

    // One common issue when running tests on Mac is that the default ulimit is too low,
    // leading to I/O errors such as "Too many open files". Raising fdlimit to bypass it.
    if let Some(limit) = fdlimit::raise_fd_limit() {
        // on windows raise_fd_limit return None
        db_options.options.set_max_open_files((limit / 8) as i32);
    }

    // The table cache is locked for updates and this determines the number
    // of shareds, ie 2^10. Increase in case of lock contentions.
    let row_cache =
        rocksdb::Cache::new_lru_cache(cache_capacity.unwrap_or(300_000)).expect("Cache is ok");
    db_options.options.set_row_cache(&row_cache);
    db_options.options.set_table_cache_num_shard_bits(10);
    db_options
        .options
        .set_compression_type(rocksdb::DBCompressionType::None);

    let mut point_lookup = db_options.clone();
    point_lookup
        .options
        .optimize_for_point_lookup(64 /* 64MB (default is 8) */);
    point_lookup.options.set_memtable_whole_key_filtering(true);

    (db_options, point_lookup)
}

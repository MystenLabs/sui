// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod lock_service;
pub use lock_service::LockService;

pub mod indexes;
pub use indexes::IndexStore;

pub mod event_store;
pub mod follower_store;
pub mod mutex_table;
pub mod write_ahead_log;

use rocksdb::Options;

/// Given a provided `db_options`, add a few default options.
/// Returns the default option and the point lookup option.
pub fn default_db_options(db_options: Option<Options>) -> (Options, Options) {
    let mut options = db_options.unwrap_or_default();

    // One common issue when running tests on Mac is that the default ulimit is too low,
    // leading to I/O errors such as "Too many open files". Raising fdlimit to bypass it.
    options.set_max_open_files((fdlimit::raise_fd_limit().unwrap() / 8) as i32);

    /* The table cache is locked for updates and this determines the number
        of shareds, ie 2^10. Increase in case of lock contentions.
    */
    let row_cache = rocksdb::Cache::new_lru_cache(300_000).expect("Cache is ok");
    options.set_row_cache(&row_cache);
    options.set_table_cache_num_shard_bits(10);
    options.set_compression_type(rocksdb::DBCompressionType::None);

    let mut point_lookup = options.clone();
    point_lookup.optimize_for_point_lookup(1024 * 1024);
    point_lookup.set_memtable_whole_key_filtering(true);

    (options, point_lookup)
}

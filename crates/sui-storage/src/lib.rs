// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod lock_service;
pub use lock_service::LockService;

pub mod indexes;
pub use indexes::IndexStore;

pub mod event_store;
pub mod mutex_table;
pub mod node_sync_store;
pub mod write_ahead_log;

use rocksdb::Options;
use std::future::Future;
use typed_store::rocks::{default_db_options as default_rocksdb_options, DBOptions};

/// Given a provided `db_options`, add a few default options.
/// Returns the default option and the point lookup option.
pub fn default_db_options(
    db_options: Option<Options>,
    cache_capacity: Option<usize>,
) -> (DBOptions, DBOptions) {
    let mut db_options = db_options
        .map(|o| DBOptions { options: o })
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
    point_lookup.options.optimize_for_point_lookup(1024 * 1024);
    point_lookup.options.set_memtable_whole_key_filtering(true);

    (db_options, point_lookup)
}

// Used to exec futures that send data to/from other threads. In the simulator, this becomes a
// blocking call, which removes the non-determinism that would otherwise be caused by the
// timing of the reply from the other thread.
//
// In production code, this should be compiled away.
pub(crate) async fn block_on_future_in_sim<F: Future>(fut: F) -> <F as Future>::Output {
    if cfg!(msim) {
        futures::executor::block_on(fut)
    } else {
        fut.await
    }
}

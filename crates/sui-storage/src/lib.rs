// Copyright (c) 2022, Mysten Labs, Inc.
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
use typed_store::rocks::default_rocksdb_options;

/// Given a provided `db_options`, add a few default options.
/// Returns the default option and the point lookup option.
pub fn default_db_options(
    db_options: Option<Options>,
    cache_capacity: Option<usize>,
) -> (Options, Options) {
    let mut options = db_options.unwrap_or_else(default_rocksdb_options);

    // One common issue when running tests on Mac is that the default ulimit is too low,
    // leading to I/O errors such as "Too many open files". Raising fdlimit to bypass it.
    if let Some(limit) = fdlimit::raise_fd_limit() {
        // on windows raise_fd_limit return None
        options.set_max_open_files((limit / 8) as i32);
    }

    // The table cache is locked for updates and this determines the number
    // of shareds, ie 2^10. Increase in case of lock contentions.
    let row_cache =
        rocksdb::Cache::new_lru_cache(cache_capacity.unwrap_or(300_000)).expect("Cache is ok");
    options.set_row_cache(&row_cache);
    options.set_table_cache_num_shard_bits(10);
    options.set_compression_type(rocksdb::DBCompressionType::None);

    let mut point_lookup = options.clone();
    point_lookup.optimize_for_point_lookup(1024 * 1024);
    point_lookup.set_memtable_whole_key_filtering(true);

    (options, point_lookup)
}

// Used to exec futures that send data to/from other threads. In the simulator, this effectively
// becomes a blocking call, which removes the non-determinism that would otherwise be caused by the
// timing of the reply from the other thread.
pub(crate) async fn exec_client_future<F: Future>(fut: F) -> <F as Future>::Output {
    if cfg!(madsim) {
        futures::executor::block_on(fut)
    } else {
        fut.await
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ffi::c_int;

use sui_default_config::DefaultConfig;

/// Configuration for setting up a RocksDB database.
#[DefaultConfig]
#[serde(deny_unknown_fields)]
pub struct DbConfig {
    /// The amount of data to keep in memory before flushing to disk, in MiB.
    pub write_buffer_size_mb: usize,

    /// Flushes will start getting forced once the WAL exceeds this size, in MiB.
    pub max_wal_size_mb: u64,

    /// The size of a single block of data in the database, in KiB.
    pub block_size_kb: usize,

    /// Number of slots in the block cache.
    pub block_cache_slots: usize,

    /// Number of threads to use for flush and compaction.
    pub db_parallelism: u32,
}

impl Default for DbConfig {
    fn default() -> Self {
        // The default values are chosen to increase resource usage compared to the default RocksDB
        // settings.
        Self {
            // Increase write buffer and WAL sizes to prepare for high write throughput.
            write_buffer_size_mb: 4 * 1024,
            max_wal_size_mb: 4 * 1024,
            // Increase block size and cache size to handle a mix of point lookup and scan
            // workloads.
            block_size_kb: 32,
            block_cache_slots: 8192,
            // Increase parallelism to allow more concurrent flushes and compactions.
            db_parallelism: 8,
        }
    }
}

impl From<DbConfig> for rocksdb::Options {
    fn from(config: DbConfig) -> Self {
        let mut opts = rocksdb::Options::default();

        opts.create_if_missing(true);

        // The Table Cache stores file descriptors for SST files, protected by locks. The locks are
        // shared between multiple cache entries, sharded by a prefix of the cache key's hash.
        //
        // https://github.com/facebook/rocksdb/wiki/RocksDB-Overview#table-cache
        opts.set_table_cache_num_shard_bits(10);

        // Compression settings
        opts.set_compression_type(rocksdb::DBCompressionType::Lz4);
        opts.set_bottommost_compression_type(rocksdb::DBCompressionType::Zstd);
        opts.set_bottommost_zstd_max_train_bytes(1024 * 1024, /* enabled */ true);

        // Writer settings
        opts.set_write_buffer_size(config.write_buffer_size_mb << 20);
        opts.set_max_total_wal_size(config.max_wal_size_mb << 20);
        opts.set_max_background_jobs(config.db_parallelism as c_int);
        opts.set_enable_pipelined_write(true);

        // Block settings
        let mut block_opts = rocksdb::BlockBasedOptions::default();
        block_opts.set_block_size(config.block_size_kb << 10);
        block_opts.set_block_cache(&rocksdb::Cache::new_lru_cache(
            (config.block_cache_slots * config.block_size_kb) << 10,
        ));

        // 1% false positive rate
        block_opts.set_bloom_filter(10.0, /* block_based */ false);

        block_opts.set_cache_index_and_filter_blocks(true);
        block_opts.set_pin_l0_filter_and_index_blocks_in_cache(true);

        opts.set_block_based_table_factory(&block_opts);

        opts
    }
}

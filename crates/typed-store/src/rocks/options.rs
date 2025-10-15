// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use rocksdb::{compaction_filter::Decision, BlockBasedOptions, Cache, MergeOperands, ReadOptions};
use std::collections::BTreeMap;
use std::env;
use tap::TapFallible;
use tracing::{info, warn};

// Write buffer size per RocksDB instance can be set via the env var below.
// If the env var is not set, use the default value in MiB.
const ENV_VAR_DB_WRITE_BUFFER_SIZE: &str = "DB_WRITE_BUFFER_SIZE_MB";
const DEFAULT_DB_WRITE_BUFFER_SIZE: usize = 1024;

// Write ahead log size per RocksDB instance can be set via the env var below.
// If the env var is not set, use the default value in MiB.
const ENV_VAR_DB_WAL_SIZE: &str = "DB_WAL_SIZE_MB";
const DEFAULT_DB_WAL_SIZE: usize = 1024;

// Environment variable to control behavior of write throughput optimized tables.
const ENV_VAR_L0_NUM_FILES_COMPACTION_TRIGGER: &str = "L0_NUM_FILES_COMPACTION_TRIGGER";
const DEFAULT_L0_NUM_FILES_COMPACTION_TRIGGER: usize = 4;
const DEFAULT_UNIVERSAL_COMPACTION_L0_NUM_FILES_COMPACTION_TRIGGER: usize = 80;
const ENV_VAR_MAX_WRITE_BUFFER_SIZE_MB: &str = "MAX_WRITE_BUFFER_SIZE_MB";
const DEFAULT_MAX_WRITE_BUFFER_SIZE_MB: usize = 256;
const ENV_VAR_MAX_WRITE_BUFFER_NUMBER: &str = "MAX_WRITE_BUFFER_NUMBER";
const DEFAULT_MAX_WRITE_BUFFER_NUMBER: usize = 6;
const ENV_VAR_TARGET_FILE_SIZE_BASE_MB: &str = "TARGET_FILE_SIZE_BASE_MB";
const DEFAULT_TARGET_FILE_SIZE_BASE_MB: usize = 128;

// Set to 1 to disable blob storage for transactions and effects.
const ENV_VAR_DISABLE_BLOB_STORAGE: &str = "DISABLE_BLOB_STORAGE";
const ENV_VAR_DB_PARALLELISM: &str = "DB_PARALLELISM";

#[derive(Clone, Debug)]
pub struct ReadWriteOptions {
    pub ignore_range_deletions: bool,
    /// When set, debug log the hash of the key and value bytes when inserting to
    /// this table.
    pub log_value_hash: bool,
}

impl ReadWriteOptions {
    pub fn readopts(&self) -> ReadOptions {
        let mut readopts = ReadOptions::default();
        readopts.set_ignore_range_deletions(self.ignore_range_deletions);
        readopts
    }

    pub fn set_ignore_range_deletions(mut self, ignore: bool) -> Self {
        self.ignore_range_deletions = ignore;
        self
    }

    pub fn set_log_value_hash(mut self, log_value_hash: bool) -> Self {
        self.log_value_hash = log_value_hash;
        self
    }
}

impl Default for ReadWriteOptions {
    fn default() -> Self {
        Self {
            ignore_range_deletions: true,
            log_value_hash: false,
        }
    }
}

#[derive(Default, Clone)]
pub struct DBOptions {
    pub options: rocksdb::Options,
    pub rw_options: ReadWriteOptions,
}

#[derive(Clone)]
pub struct DBMapTableConfigMap(BTreeMap<String, DBOptions>);
impl DBMapTableConfigMap {
    pub fn new(map: BTreeMap<String, DBOptions>) -> Self {
        Self(map)
    }

    pub fn to_map(&self) -> BTreeMap<String, DBOptions> {
        self.0.clone()
    }
}

impl DBOptions {
    // Optimize lookup perf for tables where no scans are performed.
    // If non-trivial number of values can be > 512B in size, it is beneficial to also
    // specify optimize_for_large_values_no_scan().
    pub fn optimize_for_point_lookup(mut self, block_cache_size_mb: usize) -> DBOptions {
        // NOTE: this overwrites the block options.
        self.options
            .optimize_for_point_lookup(block_cache_size_mb as u64);
        self
    }

    // Optimize write and lookup perf for tables which are rarely scanned, and have large values.
    // https://rocksdb.org/blog/2021/05/26/integrated-blob-db.html
    pub fn optimize_for_large_values_no_scan(mut self, min_blob_size: u64) -> DBOptions {
        if env::var(ENV_VAR_DISABLE_BLOB_STORAGE).is_ok() {
            info!("Large value blob storage optimization is disabled via env var.");
            return self;
        }

        // Blob settings.
        self.options.set_enable_blob_files(true);
        self.options
            .set_blob_compression_type(rocksdb::DBCompressionType::Lz4);
        self.options.set_enable_blob_gc(true);
        // Since each blob can have non-trivial size overhead, and compression does not work across blobs,
        // set a min blob size in bytes to so small transactions and effects are kept in sst files.
        self.options.set_min_blob_size(min_blob_size);

        // Increase write buffer size to 256MiB.
        let write_buffer_size = read_size_from_env(ENV_VAR_MAX_WRITE_BUFFER_SIZE_MB)
            .unwrap_or(DEFAULT_MAX_WRITE_BUFFER_SIZE_MB)
            * 1024
            * 1024;
        self.options.set_write_buffer_size(write_buffer_size);
        // Since large blobs are not in sst files, reduce the target file size and base level
        // target size.
        let target_file_size_base = 64 << 20;
        self.options
            .set_target_file_size_base(target_file_size_base);
        // Level 1 default to 64MiB * 4 ~ 256MiB.
        let max_level_zero_file_num = read_size_from_env(ENV_VAR_L0_NUM_FILES_COMPACTION_TRIGGER)
            .unwrap_or(DEFAULT_L0_NUM_FILES_COMPACTION_TRIGGER);
        self.options
            .set_max_bytes_for_level_base(target_file_size_base * max_level_zero_file_num as u64);

        self
    }

    // Optimize tables with a mix of lookup and scan workloads.
    pub fn optimize_for_read(mut self, block_cache_size_mb: usize) -> DBOptions {
        self.options
            .set_block_based_table_factory(&get_block_options(block_cache_size_mb, 16 << 10));
        self
    }

    // Optimize DB receiving significant insertions.
    pub fn optimize_db_for_write_throughput(mut self, db_max_write_buffer_gb: u64) -> DBOptions {
        self.options
            .set_db_write_buffer_size(db_max_write_buffer_gb as usize * 1024 * 1024 * 1024);
        self.options
            .set_max_total_wal_size(db_max_write_buffer_gb * 1024 * 1024 * 1024);
        self
    }

    // Optimize tables receiving significant insertions.
    pub fn optimize_for_write_throughput(mut self) -> DBOptions {
        // Increase write buffer size to 256MiB.
        let write_buffer_size = read_size_from_env(ENV_VAR_MAX_WRITE_BUFFER_SIZE_MB)
            .unwrap_or(DEFAULT_MAX_WRITE_BUFFER_SIZE_MB)
            * 1024
            * 1024;
        self.options.set_write_buffer_size(write_buffer_size);
        // Increase write buffers to keep to 6 before slowing down writes.
        let max_write_buffer_number = read_size_from_env(ENV_VAR_MAX_WRITE_BUFFER_NUMBER)
            .unwrap_or(DEFAULT_MAX_WRITE_BUFFER_NUMBER);
        self.options
            .set_max_write_buffer_number(max_write_buffer_number.try_into().unwrap());
        // Keep 1 write buffer so recent writes can be read from memory.
        self.options
            .set_max_write_buffer_size_to_maintain((write_buffer_size).try_into().unwrap());

        // Increase compaction trigger for level 0 to 6.
        let max_level_zero_file_num = read_size_from_env(ENV_VAR_L0_NUM_FILES_COMPACTION_TRIGGER)
            .unwrap_or(DEFAULT_L0_NUM_FILES_COMPACTION_TRIGGER);
        self.options.set_level_zero_file_num_compaction_trigger(
            max_level_zero_file_num.try_into().unwrap(),
        );
        self.options.set_level_zero_slowdown_writes_trigger(
            (max_level_zero_file_num * 12).try_into().unwrap(),
        );
        self.options
            .set_level_zero_stop_writes_trigger((max_level_zero_file_num * 16).try_into().unwrap());

        // Increase sst file size to 128MiB.
        self.options.set_target_file_size_base(
            read_size_from_env(ENV_VAR_TARGET_FILE_SIZE_BASE_MB)
                .unwrap_or(DEFAULT_TARGET_FILE_SIZE_BASE_MB) as u64
                * 1024
                * 1024,
        );

        // Increase level 1 target size to 256MiB * 6 ~ 1.5GiB.
        self.options
            .set_max_bytes_for_level_base((write_buffer_size * max_level_zero_file_num) as u64);

        self
    }

    // Optimize tables receiving significant insertions, without any deletions.
    // TODO: merge this function with optimize_for_write_throughput(), and use a flag to
    // indicate if deletion is received.
    pub fn optimize_for_write_throughput_no_deletion(mut self) -> DBOptions {
        // Increase write buffer size to 256MiB.
        let write_buffer_size = read_size_from_env(ENV_VAR_MAX_WRITE_BUFFER_SIZE_MB)
            .unwrap_or(DEFAULT_MAX_WRITE_BUFFER_SIZE_MB)
            * 1024
            * 1024;
        self.options.set_write_buffer_size(write_buffer_size);
        // Increase write buffers to keep to 6 before slowing down writes.
        let max_write_buffer_number = read_size_from_env(ENV_VAR_MAX_WRITE_BUFFER_NUMBER)
            .unwrap_or(DEFAULT_MAX_WRITE_BUFFER_NUMBER);
        self.options
            .set_max_write_buffer_number(max_write_buffer_number.try_into().unwrap());
        // Keep 1 write buffer so recent writes can be read from memory.
        self.options
            .set_max_write_buffer_size_to_maintain((write_buffer_size).try_into().unwrap());

        // Switch to universal compactions.
        self.options
            .set_compaction_style(rocksdb::DBCompactionStyle::Universal);
        let mut compaction_options = rocksdb::UniversalCompactOptions::default();
        compaction_options.set_max_size_amplification_percent(10000);
        compaction_options.set_stop_style(rocksdb::UniversalCompactionStopStyle::Similar);
        self.options
            .set_universal_compaction_options(&compaction_options);

        let max_level_zero_file_num = read_size_from_env(ENV_VAR_L0_NUM_FILES_COMPACTION_TRIGGER)
            .unwrap_or(DEFAULT_UNIVERSAL_COMPACTION_L0_NUM_FILES_COMPACTION_TRIGGER);
        self.options.set_level_zero_file_num_compaction_trigger(
            max_level_zero_file_num.try_into().unwrap(),
        );
        self.options.set_level_zero_slowdown_writes_trigger(
            (max_level_zero_file_num * 12).try_into().unwrap(),
        );
        self.options
            .set_level_zero_stop_writes_trigger((max_level_zero_file_num * 16).try_into().unwrap());

        // Increase sst file size to 128MiB.
        self.options.set_target_file_size_base(
            read_size_from_env(ENV_VAR_TARGET_FILE_SIZE_BASE_MB)
                .unwrap_or(DEFAULT_TARGET_FILE_SIZE_BASE_MB) as u64
                * 1024
                * 1024,
        );

        // This should be a no-op for universal compaction but increasing it to be safe.
        self.options
            .set_max_bytes_for_level_base((write_buffer_size * max_level_zero_file_num) as u64);

        self
    }

    // Overrides the block options with different block cache size and block size.
    pub fn set_block_options(
        mut self,
        block_cache_size_mb: usize,
        block_size_bytes: usize,
    ) -> DBOptions {
        self.options
            .set_block_based_table_factory(&get_block_options(
                block_cache_size_mb,
                block_size_bytes,
            ));
        self
    }

    // Disables write stalling and stopping based on pending compaction bytes.
    pub fn disable_write_throttling(mut self) -> DBOptions {
        self.options.set_soft_pending_compaction_bytes_limit(0);
        self.options.set_hard_pending_compaction_bytes_limit(0);
        self.options.set_level_zero_slowdown_writes_trigger(512);
        self.options.set_level_zero_stop_writes_trigger(1024);
        self
    }

    pub fn set_merge_operator_associative<F>(mut self, name: &str, merge_fn: F) -> DBOptions
    where
        F: Fn(&[u8], Option<&[u8]>, &MergeOperands) -> Option<Vec<u8>>
            + Send
            + Sync
            + Clone
            + 'static,
    {
        self.options.set_merge_operator_associative(name, merge_fn);
        self
    }

    pub fn set_compaction_filter<F>(mut self, name: &str, filter_fn: F) -> DBOptions
    where
        F: FnMut(u32, &[u8], &[u8]) -> Decision + Send + 'static,
    {
        self.options.set_compaction_filter(name, filter_fn);
        self
    }
}

/// Creates a default RocksDB option, to be used when RocksDB option is unspecified.
pub fn default_db_options() -> DBOptions {
    let mut opt = rocksdb::Options::default();

    // One common issue when running tests on Mac is that the default ulimit is too low,
    // leading to I/O errors such as "Too many open files". Raising fdlimit to bypass it.
    if let Some(limit) = fdlimit::raise_fd_limit() {
        // on windows raise_fd_limit return None
        opt.set_max_open_files((limit / 8) as i32);
    }

    // The table cache is locked for updates and this determines the number
    // of shards, ie 2^10. Increase in case of lock contentions.
    opt.set_table_cache_num_shard_bits(10);

    // LSM compression settings
    opt.set_compression_type(rocksdb::DBCompressionType::Lz4);
    opt.set_bottommost_compression_type(rocksdb::DBCompressionType::Zstd);
    opt.set_bottommost_zstd_max_train_bytes(1024 * 1024, true);

    // Sui uses multiple RocksDB in a node, so total sizes of write buffers and WAL can be higher
    // than the limits below.
    //
    // RocksDB also exposes the option to configure total write buffer size across multiple instances
    // via `write_buffer_manager`. But the write buffer flush policy (flushing the buffer receiving
    // the next write) may not work well. So sticking to per-db write buffer size limit for now.
    //
    // The environment variables are only meant to be emergency overrides. They may go away in future.
    // It is preferable to update the default value, or override the option in code.
    opt.set_db_write_buffer_size(
        read_size_from_env(ENV_VAR_DB_WRITE_BUFFER_SIZE).unwrap_or(DEFAULT_DB_WRITE_BUFFER_SIZE)
            * 1024
            * 1024,
    );
    opt.set_max_total_wal_size(
        read_size_from_env(ENV_VAR_DB_WAL_SIZE).unwrap_or(DEFAULT_DB_WAL_SIZE) as u64 * 1024 * 1024,
    );

    // Num threads for compactions and memtable flushes.
    opt.increase_parallelism(read_size_from_env(ENV_VAR_DB_PARALLELISM).unwrap_or(8) as i32);

    opt.set_enable_pipelined_write(true);

    // Increase block size to 16KiB.
    // https://github.com/EighteenZi/rocksdb_wiki/blob/master/Memory-usage-in-RocksDB.md#indexes-and-filter-blocks
    opt.set_block_based_table_factory(&get_block_options(128, 16 << 10));

    // Set memtable bloomfilter.
    opt.set_memtable_prefix_bloom_ratio(0.02);

    DBOptions {
        options: opt,
        rw_options: ReadWriteOptions::default(),
    }
}

fn get_block_options(block_cache_size_mb: usize, block_size_bytes: usize) -> BlockBasedOptions {
    // Set options mostly similar to those used in optimize_for_point_lookup(),
    // except non-default binary and hash index, to hopefully reduce lookup latencies
    // without causing any regression for scanning, with slightly more memory usages.
    // https://github.com/facebook/rocksdb/blob/11cb6af6e5009c51794641905ca40ce5beec7fee/options/options.cc#L611-L621
    let mut block_options = BlockBasedOptions::default();
    // Overrides block size.
    block_options.set_block_size(block_size_bytes);
    // Configure a block cache.
    block_options.set_block_cache(&Cache::new_lru_cache(block_cache_size_mb << 20));
    // Set a bloomfilter with 1% false positive rate.
    block_options.set_bloom_filter(10.0, false);
    // From https://github.com/EighteenZi/rocksdb_wiki/blob/master/Block-Cache.md#caching-index-and-filter-blocks
    block_options.set_pin_l0_filter_and_index_blocks_in_cache(true);
    block_options
}

pub fn read_size_from_env(var_name: &str) -> Option<usize> {
    env::var(var_name)
        .ok()?
        .parse::<usize>()
        .tap_err(|e| {
            warn!(
                "Env var {} does not contain valid usize integer: {}",
                var_name, e
            )
        })
        .ok()
}

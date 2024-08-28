// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use once_cell::sync::OnceCell;

/// The minimum and maximum schema versions supported by this build.
const MIN_SCHEMA_VERSION: u64 = 1;
pub const MAX_SCHEMA_VERSION: u64 = 2;

// Record history of schema version allocations here:
// 1: Initial version, cut on 2024-07-13-003534.
// 2: ...

static CUR_SCHEMA_VERSION: OnceCell<Option<u64>> = OnceCell::new();
static CUR_SCHEMA_CONFIG: OnceCell<Option<IndexerSchemaConfig>> = OnceCell::new();

pub fn set_schema_version_at_startup(version: u64) {
    CUR_SCHEMA_VERSION.set(Some(version)).unwrap();
    let config = IndexerSchemaConfig::get_for_version(version);
    CUR_SCHEMA_CONFIG.set(Some(config)).unwrap();
}

pub fn get_schema_config() -> &'static IndexerSchemaConfig {
    CUR_SCHEMA_CONFIG.get().unwrap().as_ref().unwrap()
}

#[derive(Debug)]
struct FeatureFlags {}

#[derive(Debug)]
pub struct IndexerSchemaConfig {
    #[allow(dead_code)]
    feature_flags: FeatureFlags,
    last_schema_migration: &'static str,
}

impl IndexerSchemaConfig {
    pub fn get_for_version(version: u64) -> Self {
        assert!(
            version >= MIN_SCHEMA_VERSION,
            "Indexer protocol version is {:?}, but the minimum supported version by the binary is {:?}. Please upgrade the binary.",
            version,
            MIN_SCHEMA_VERSION,
        );
        assert!(
            version <= MAX_SCHEMA_VERSION,
            "Indexer protocol version is {:?}, but the maximum supported version by the binary is {:?}. Please upgrade the binary.",
            version,
            MAX_SCHEMA_VERSION,
        );

        let mut cfg = Self {
            feature_flags: FeatureFlags {},
            last_schema_migration: "",
        };
        for cur in 1..=version {
            match cur {
                1 => {
                    // This version is frozen.
                    cfg.last_schema_migration = "2024-07-13-003534_chain_identifier";
                }
                2 => {
                    // Add new feature flags here.
                }
                _ => panic!("unsupported version {:?}", version),
            }
        }
        cfg
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub const CUR_SCHEMA_VERSION: u64 = 1;
pub const NEXT_SCHEMA_VERSION: u64 = 2;

// Record history of schema version allocations here:
// 1: Initial version, cut on 2024-07-13-003534.
// 2: ...

#[derive(Debug, Clone)]
struct FeatureFlags {}

#[derive(Debug, Clone)]
pub struct IndexerSchemaConfig {
    #[allow(dead_code)]
    feature_flags: FeatureFlags,
    last_schema_migration: &'static str,
}

impl IndexerSchemaConfig {
    pub fn last_schema_migration(&self) -> &'static str {
        self.last_schema_migration
    }

    pub fn get_for_version(version: u64) -> Self {
        assert!(
            version >= CUR_SCHEMA_VERSION,
            "Indexer protocol version is {:?}, but the minimum supported version by the binary is {:?}. Please upgrade the binary.",
            version,
            CUR_SCHEMA_VERSION,
        );
        assert!(
            version <= NEXT_SCHEMA_VERSION,
            "Indexer protocol version is {:?}, but the maximum supported version by the binary is {:?}. Please upgrade the binary.",
            version,
            NEXT_SCHEMA_VERSION,
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
                    // Add new feature flags here and update the last schema migration.
                    cfg.last_schema_migration = "2024-07-13-003534_chain_identifier";
                }
                _ => panic!("unsupported version {:?}", version),
            }
        }
        cfg
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

const DEFAULT_DEPTH_LIMIT: usize = 10;
const OVERALL_COMPLEXITY_LIMIT: usize = 1000;
const RECURSIVE_DEPTH_LIMIT: usize = 16;

const DEFAULT_BASE_COMPLEXITY: usize = 3;
const DEFAULT_CHILD_COMPLEXITY_MULTIPLIER: usize = 2;
const DEFAULT_CONNECTION_COMPLEXITY_MULTIPLIER: usize = 5;

static COMPLEXITY_CONFIG: OnceLock<ComplexityConfig> = OnceLock::new();

#[derive(Serialize, Deserialize, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Copy, Debug)]
pub struct ComplexityConfigEntry {
    pub base_complexity: usize,
    pub child_complexity_multiplier: usize,
    pub connection_complexity_multiplier: usize,
}

impl Default for ComplexityConfigEntry {
    fn default() -> Self {
        Self {
            base_complexity: DEFAULT_BASE_COMPLEXITY,
            child_complexity_multiplier: DEFAULT_CHILD_COMPLEXITY_MULTIPLIER,
            connection_complexity_multiplier: DEFAULT_CONNECTION_COMPLEXITY_MULTIPLIER,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Copy, Debug)]
pub struct ComplexityConfig {
    // Overall depth limit
    pub depth_limit: usize,
    // Overall complexity limit
    pub complexity_limit: usize,
    // Recursive depth limit
    pub recursive_depth_limit: usize,

    // Query level
    pub chain_identifier: ComplexityConfigEntry,
    pub owner: ComplexityConfigEntry,
    pub object: ComplexityConfigEntry,
    pub address: ComplexityConfigEntry,
    pub checkpoint_connection: ComplexityConfigEntry,
    pub protocol_config: ComplexityConfigEntry,
    // TODO: add complexities other queries & types
}

impl Default for ComplexityConfig {
    fn default() -> Self {
        Self {
            depth_limit: DEFAULT_DEPTH_LIMIT,
            complexity_limit: OVERALL_COMPLEXITY_LIMIT,
            recursive_depth_limit: RECURSIVE_DEPTH_LIMIT,
            chain_identifier: ComplexityConfigEntry::default(),
            owner: ComplexityConfigEntry::default(),
            object: ComplexityConfigEntry::default(),
            address: ComplexityConfigEntry::default(),
            checkpoint_connection: ComplexityConfigEntry::default(),
            protocol_config: ComplexityConfigEntry::default(),
        }
    }
}

/// Standard calculation for complexity
pub(crate) fn standard_calc(
    c: &ComplexityConfigEntry,
    first: Option<u64>,
    last: Option<u64>,
    child_complexity: usize,
) -> usize {
    let mut complexity = c.base_complexity;
    if let Some(first) = first {
        complexity += first as usize * c.connection_complexity_multiplier;
    }
    if let Some(last) = last {
        complexity += last as usize * c.connection_complexity_multiplier;
    }
    complexity += child_complexity * c.child_complexity_multiplier;
    complexity
}

pub(crate) fn get_complexity_config() -> &'static ComplexityConfig {
    COMPLEXITY_CONFIG
        .get()
        .expect("complexity config value must be set before use")
}

pub(crate) fn set_complexity_config(config: &ComplexityConfig) {
    COMPLEXITY_CONFIG
        .set(*config)
        .expect("complexity config value can only be set once");
}

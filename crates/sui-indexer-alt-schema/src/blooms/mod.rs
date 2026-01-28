// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_types::SUI_CLOCK_OBJECT_ID;
use sui_types::base_types::ObjectID;

pub mod blocked;
pub mod bloom;
pub mod hash;

/// Addresses to skip during bloom filter operations. These appear in most
/// checkpoints, so including them would:
/// - Cause queries to match nearly all blocks and checkpoints (not useful)
/// - Require fetching and probing more bloom filter rows at both levels
const BLOOM_SKIP_ADDRESSES: &[ObjectID] = &[ObjectID::ZERO, SUI_CLOCK_OBJECT_ID];

/// Returns true if the given bytes represent an address that should be skipped
/// for bloom filter operations (appears in most checkpoints).
pub fn should_skip_for_bloom(bytes: &[u8]) -> bool {
    BLOOM_SKIP_ADDRESSES.iter().any(|id| id.as_ref() == bytes)
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::account_address::AccountAddress;
use sui_types::SUI_CLOCK_ADDRESS;

pub mod blocked;
pub mod bloom;
pub mod hash;

/// High-frequency identifiers excluded from bloom filters. These appear in most
/// checkpoints, so including them would:
/// - Cause queries to match nearly all blocks and checkpoints
/// - Require fetching and probing more bloom filter rows at both levels
const BLOOM_SKIP_ADDRESSES: &[AccountAddress] = &[AccountAddress::ZERO, SUI_CLOCK_ADDRESS];

/// Returns true if the bytes should be excluded from bloom filter operations.
pub fn should_skip_for_bloom(bytes: &[u8]) -> bool {
    BLOOM_SKIP_ADDRESSES.iter().any(|id| id.as_ref() == bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_skip_for_bloom() {
        assert!(should_skip_for_bloom(AccountAddress::ZERO.as_ref()));
        assert!(should_skip_for_bloom(SUI_CLOCK_ADDRESS.as_ref()));

        let mut bytes = [0u8; AccountAddress::LENGTH];
        bytes[0] = 0x42;
        assert!(!should_skip_for_bloom(&bytes));
    }
}

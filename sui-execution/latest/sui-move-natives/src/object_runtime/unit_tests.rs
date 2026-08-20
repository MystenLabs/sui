// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::ObjectFundsAvailable;
use move_core_types::u256::U256;

#[test]
fn store_read_is_needed_only_when_uncached_balance_is_insufficient() {
    let empty = ObjectFundsAvailable::default();
    assert!(!empty.needs_store_read(U256::from(0u64)));
    assert!(empty.needs_store_read(U256::from(1u64)));

    let deposited = ObjectFundsAvailable {
        available: U256::from(500u64),
        queried: false,
    };
    assert!(!deposited.needs_store_read(U256::from(500u64)));
    assert!(deposited.needs_store_read(U256::from(501u64)));

    let queried = ObjectFundsAvailable {
        available: U256::from(0u64),
        queried: true,
    };
    assert!(!queried.needs_store_read(U256::from(1u64)));
}

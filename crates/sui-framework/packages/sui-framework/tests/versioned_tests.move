// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::versioned_tests {
    use sui::tx_context;
    use sui::versioned;

    #[test]
    fun test_upgrade() {
        let ctx = tx_context::dummy();
        let wrapper = versioned::create(1, 1000, &mut ctx);
        assert!(versioned::version(&wrapper) == 1, 0);
        assert!(versioned::load_value(&wrapper) == &1000, 0);
        let (old, cap) = versioned::remove_value_for_upgrade(&mut wrapper);
        assert!(old == 1000, 0);
        versioned::upgrade(&mut wrapper, 2, 2000, cap);
        assert!(versioned::version(&wrapper) == 2, 0);
        assert!(versioned::load_value(&wrapper) == &2000, 0);
        let value = versioned::destroy(wrapper);
        assert!(value == 2000, 0);
    }
}
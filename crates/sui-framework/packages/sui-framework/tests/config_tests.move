// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::config_tests {

    use sui::config::{Self, Config};
    use sui::test_scenario as ts;

    const SENDER: address = @42;

    public struct WriteCap() has drop;
    public struct Wrapped<T>(T) has copy, drop, store;

    fun new(ctx: &mut TxContext): Config<WriteCap> {
        config::new(&mut WriteCap(), ctx)
    }

    #[test]
    fun test_all() {
        let mut ts = ts::begin(SENDER);
        let mut config = new(ts.ctx());
        let n1 = b"hello";
        let n2 = Wrapped(b"hello");
        assert!(!config.exists_with_type<_, _, u8>(n1));
        assert!(!config.exists_with_type_for_epoch<_, _, u8>(n1, ts.ctx()));
        assert!(!config.exists_with_type<_, _, u8>(n2));
        assert!(!config.exists_with_type_for_epoch<_, _, u8>(n2, ts.ctx()));

        // epoch0
        // n1 -- epoch0 --> 112
        config.new_for_epoch(&mut WriteCap(), n1, 112u8, ts.ctx());
        assert!(config.exists_with_type<_, _, u8>(n1));
        assert!(config.exists_with_type_for_epoch<_, _, u8>(n1, ts.ctx()));
        assert!(!config.exists_with_type<_, _, u8>(n2));
        assert!(!config.exists_with_type_for_epoch<_, _, u8>(n2, ts.ctx()));
        assert!(config.borrow_for_epoch_mut(&mut WriteCap(), n1, ts.ctx()) == 112u8);
        assert!(config.borrow_most_recent(n1) == 112u8);

        // epoch0
        // n1 -- epoch0 --> 224
        *config.borrow_for_epoch_mut(&mut WriteCap(), n1, ts.ctx()) = 224u8;
        assert!(config.borrow_for_epoch_mut(&mut WriteCap(), n1, ts.ctx()) == 224u8);
        assert!(config.borrow_most_recent(n1) == 224u8);

        // epoch1
        // n1 -- epoch0 --> 224
        ts.next_epoch(SENDER);
        assert!(config.exists_with_type<_, _, u8>(n1));
        assert!(!config.exists_with_type_for_epoch<_, _, u8>(n1, ts.ctx()));
        assert!(!config.exists_with_type<_, _, u8>(n2));
        assert!(!config.exists_with_type_for_epoch<_, _, u8>(n2, ts.ctx()));
        assert!(config.borrow_most_recent(n1) == 224u8);

        // epoch1
        // n1 -- epoch0 --> 224
        // n1 -- epoch1 --> 0
        config.new_for_epoch(&mut WriteCap(), n1, 0u8, ts.ctx());
        assert!(config.exists_with_type<_, _, u8>(n1));
        assert!(config.exists_with_type_for_epoch<_, _, u8>(n1, ts.ctx()));
        assert!(!config.exists_with_type<_, _, u8>(n2));
        assert!(!config.exists_with_type_for_epoch<_, _, u8>(n2, ts.ctx()));
        assert!(config.borrow_for_epoch_mut(&mut WriteCap(), n1, ts.ctx()) == 0u8);
        assert!(config.borrow_most_recent(n1) == 0u8);

        // epoch1
        // n1 -- epoch0 --> 224
        // n1 -- epoch1 --> 0
        // n2 -- epoch1 --> 2
        config.new_for_epoch(&mut WriteCap(), n2, 2u8, ts.ctx());
        assert!(config.exists_with_type<_, _, u8>(n1));
        assert!(config.exists_with_type_for_epoch<_, _, u8>(n1, ts.ctx()));
        assert!(config.exists_with_type<_, _, u8>(n2));
        assert!(config.exists_with_type_for_epoch<_, _, u8>(n2, ts.ctx()));
        assert!(config.borrow_for_epoch_mut(&mut WriteCap(), n1, ts.ctx()) == 0u8);
        assert!(config.borrow_most_recent(n1) == 0u8);
        assert!(config.borrow_for_epoch_mut(&mut WriteCap(), n2, ts.ctx()) == 2u8);
        assert!(config.borrow_most_recent(n2) == 2u8);

        // epoch2
        // n1 -- epoch0 --> 224
        // n1 -- epoch1 --> 0
        // n2 -- epoch1 --> 2
        ts.next_epoch(SENDER);
        assert!(config.exists_with_type<_, _, u8>(n1));
        assert!(!config.exists_with_type_for_epoch<_, _, u8>(n1, ts.ctx()));
        assert!(config.exists_with_type<_, _, u8>(n2));
        assert!(!config.exists_with_type_for_epoch<_, _, u8>(n2, ts.ctx()));
        assert!(config.borrow_most_recent(n1) == 0u8);
        assert!(config.borrow_most_recent(n2) == 2u8);

        config::destroy(config);
        ts.end();
    }

    #[test, expected_failure(abort_code = sui::config::EAlreadySetForEpoch)]
    fun new_for_epoch_aborts_in_same_epoch() {
        let mut ts = ts::begin(SENDER);
        let mut config = new(ts.ctx());
        config.new_for_epoch(&mut WriteCap(), false, 0u8, ts.ctx());
        config.new_for_epoch(&mut WriteCap(), false, 1u8, ts.ctx());
        abort 0
    }

    #[test, expected_failure(abort_code = sui::config::ENotSetForEpoch)]
    fun borrow_for_epoch_mut_aborts_in_new_epoch() {
        let mut ts = ts::begin(SENDER);
        let mut config = new(ts.ctx());
        let n = 1u64;
        config.new_for_epoch(&mut WriteCap(), n, b"hello", ts.ctx());
        assert!(config.exists_with_type<_, _, vector<u8>>(n));
        assert!(config.exists_with_type_for_epoch<_, _, vector<u8>>(n, ts.ctx()));
        assert!(config.borrow_most_recent(n) == b"hello");

        ts.next_epoch(SENDER);
        assert!(config.exists_with_type<_, _, vector<u8>>(n));
        assert!(!config.exists_with_type_for_epoch<_, _, vector<u8>>(n, ts.ctx()));
        assert!(config.borrow_most_recent(n) == b"hello");
        // aborts
        config.borrow_for_epoch_mut<_, _, vector<u8>>(&mut WriteCap(), n, ts.ctx());
        abort 0
    }


}

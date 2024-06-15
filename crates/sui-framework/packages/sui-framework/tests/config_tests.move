// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::config_tests {

    use sui::config::{Self, Config};
    use sui::test_scenario as ts;

    const SENDER: address = @42;

    public struct WriteCap() has drop;
    public struct Wrapped<T>(T) has copy, drop, store;

    #[test]
    fun test_all() {
        let mut ts = ts::begin(SENDER);
        config::create(&mut WriteCap(), ts.ctx());
        ts.next_tx(SENDER);
        let id = ts::most_recent_id_shared<Config<WriteCap>>().destroy_some();
        let n1 = b"hello";
        let n2 = Wrapped(b"hello");
        {
            let mut config: Config<WriteCap> = ts.take_shared_by_id(id);
            assert!(!config.exists_with_type<_, _, u8>(n1));
            assert!(!config.exists_with_type_for_epoch<_, _, u8>(n1, ts.ctx()));
            assert!(!config.exists_with_type<_, _, u8>(n2));
            assert!(!config.exists_with_type_for_epoch<_, _, u8>(n2, ts.ctx()));
            assert!(config::read_setting<_, u8>(id, n1, ts.ctx()).is_none());
            assert!(config::read_setting<_, u8>(id, n2, ts.ctx()).is_none());

            // epoch0
            // n1 -- epoch0 --> 112
            config.new_for_epoch(&mut WriteCap(), n1, 112u8, ts.ctx());
            assert!(config.exists_with_type<_, _, u8>(n1));
            assert!(config.exists_with_type_for_epoch<_, _, u8>(n1, ts.ctx()));
            assert!(!config.exists_with_type<_, _, u8>(n2));
            assert!(!config.exists_with_type_for_epoch<_, _, u8>(n2, ts.ctx()));
            assert!(config.borrow_for_epoch_mut(&mut WriteCap(), n1, ts.ctx()) == 112u8);
            assert!(config.borrow_most_recent(n1) == 112u8);
            assert!(config::read_setting<_, u8>(id, n1, ts.ctx()).is_none());
            assert!(config::read_setting<_, u8>(id, n2, ts.ctx()).is_none());

            // epoch0
            // n1 -- epoch0 --> 224
            *config.borrow_for_epoch_mut(&mut WriteCap(), n1, ts.ctx()) = 224u8;
            assert!(config.borrow_for_epoch_mut(&mut WriteCap(), n1, ts.ctx()) == 224u8);
            assert!(config.borrow_most_recent(n1) == 224u8);
            assert!(config::read_setting<_, u8>(id, n1, ts.ctx()).is_none());
            assert!(config::read_setting<_, u8>(id, n2, ts.ctx()).is_none());
            ts::return_shared(config);
        };

        ts.next_epoch(SENDER);
        {
            let mut config: Config<WriteCap> = ts.take_shared_by_id(id);
            // epoch1
            // n1 -- epoch0 --> 224
            assert!(config.exists_with_type<_, _, u8>(n1));
            assert!(!config.exists_with_type_for_epoch<_, _, u8>(n1, ts.ctx()));
            assert!(!config.exists_with_type<_, _, u8>(n2));
            assert!(!config.exists_with_type_for_epoch<_, _, u8>(n2, ts.ctx()));
            assert!(config.borrow_most_recent(n1) == 224u8);
            assert!(config::read_setting<_, u8>(id, n1, ts.ctx()).destroy_some() == 224u8);
            assert!(config::read_setting<_, u8>(id, n2, ts.ctx()).is_none());

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
            assert!(config::read_setting<_, u8>(id, n1, ts.ctx()).destroy_some() == 224u8);
            assert!(config::read_setting<_, u8>(id, n2, ts.ctx()).is_none());

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
            assert!(config::read_setting<_, u8>(id, n1, ts.ctx()).destroy_some() == 224u8);
            assert!(config::read_setting<_, u8>(id, n2, ts.ctx()).is_none());
            ts::return_shared(config);
        };

        // check that read_setting is not updated until next epoch
        ts.next_tx(SENDER);
        assert!(config::read_setting<_, u8>(id, n1, ts.ctx()).destroy_some() == 224u8);
        assert!(config::read_setting<_, u8>(id, n2, ts.ctx()).is_none());

        ts.next_epoch(SENDER);
        assert!(config::read_setting<_, u8>(id, n1, ts.ctx()).destroy_some() == 0u8);
        assert!(config::read_setting<_, u8>(id, n2, ts.ctx()).destroy_some() == 2u8);

        ts.next_tx(SENDER);
        {
            let config: Config<WriteCap> = ts.take_shared_by_id(id);
            // epoch2
            // n1 -- epoch0 --> 224
            // n1 -- epoch1 --> 0
            // n2 -- epoch1 --> 2
            assert!(config.exists_with_type<_, _, u8>(n1));
            assert!(!config.exists_with_type_for_epoch<_, _, u8>(n1, ts.ctx()));
            assert!(config.exists_with_type<_, _, u8>(n2));
            assert!(!config.exists_with_type_for_epoch<_, _, u8>(n2, ts.ctx()));
            assert!(config.borrow_most_recent(n1) == 0u8);
            assert!(config.borrow_most_recent(n2) == 2u8);
            assert!(config::read_setting<_, u8>(id, n1, ts.ctx()).destroy_some() == 0u8);
            assert!(config::read_setting<_, u8>(id, n2, ts.ctx()).destroy_some() == 2u8);
            ts::return_shared(config);
        };

        ts.end();
    }

    #[test, expected_failure(abort_code = sui::config::EAlreadySetForEpoch)]
    fun new_for_epoch_aborts_in_same_epoch() {
        let mut ts = ts::begin(SENDER);
        config::create(&mut WriteCap(), ts.ctx());
        ts.next_tx(SENDER);
        let mut config: Config<WriteCap> = ts.take_shared();
        config.new_for_epoch(&mut WriteCap(), false, 0u8, ts.ctx());
        config.new_for_epoch(&mut WriteCap(), false, 1u8, ts.ctx());
        abort 0
    }

    #[test, expected_failure(abort_code = sui::config::ENotSetForEpoch)]
    fun borrow_for_epoch_mut_aborts_in_new_epoch() {
        let mut ts = ts::begin(SENDER);
        config::create(&mut WriteCap(), ts.ctx());
        ts.next_tx(SENDER);
        let mut config: Config<WriteCap> = ts.take_shared();
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

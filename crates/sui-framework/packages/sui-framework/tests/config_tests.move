// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::config_tests {

    use sui::config::{Self, Config};
    use sui::test_scenario as ts;
    use std::unit_test::assert_eq;

    const SENDER: address = @42;

    public struct WriteCap() has drop;
    public struct Wrapped<T>(T) has copy, drop, store;

    fun config_create<WriteCap>(cap: &mut WriteCap, ctx: &mut TxContext) {
        sui::config::share(sui::config::new(cap, ctx))
    }

    #[test]
    fun test_all() {
        let mut ts = ts::begin(SENDER);
        config_create(&mut WriteCap(), ts.ctx());
        ts.next_tx(SENDER);
        let id = ts::most_recent_id_shared<Config<WriteCap>>().destroy_some();
        let n1 = b"hello";
        let n2 = Wrapped(b"hello");
        {
            let mut config: Config<WriteCap> = ts.take_shared_by_id(id);
            assert!(!config.exists_with_type<_, _, u8>(n1));
            assert!(!config.exists_with_type_for_next_epoch<_, _, u8>(n1, ts.ctx()));
            assert!(!config.exists_with_type<_, _, u8>(n2));
            assert!(!config.exists_with_type_for_next_epoch<_, _, u8>(n2, ts.ctx()));
            assert!(config::read_setting<_, u8>(id, n1, ts.ctx()).is_none());
            assert!(config::read_setting<_, u8>(id, n2, ts.ctx()).is_none());

            // epoch0
            // n1 -- epoch0 --> 112
            config.add_for_next_epoch(&mut WriteCap(), n1, 112u8, ts.ctx());
            assert!(config.exists_with_type<_, _, u8>(n1));
            assert!(config.exists_with_type_for_next_epoch<_, _, u8>(n1, ts.ctx()));
            assert!(!config.exists_with_type<_, _, u8>(n2));
            assert!(!config.exists_with_type_for_next_epoch<_, _, u8>(n2, ts.ctx()));
            assert!(config.borrow_for_next_epoch_mut(&mut WriteCap(), n1, ts.ctx()) == 112u8);
            assert!(config.read_setting_for_next_epoch(n1) == option::some(112u8));
            assert!(config::read_setting<_, u8>(id, n1, ts.ctx()).is_none());
            assert!(config::read_setting<_, u8>(id, n2, ts.ctx()).is_none());

            // epoch0
            // n1 -- epoch0 --> 224
            *config.borrow_for_next_epoch_mut(&mut WriteCap(), n1, ts.ctx()) = 224u8;
            assert!(config.borrow_for_next_epoch_mut(&mut WriteCap(), n1, ts.ctx()) == 224u8);
            assert!(config.read_setting_for_next_epoch(n1) ==  option::some(224u8));
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
            assert!(!config.exists_with_type_for_next_epoch<_, _, u8>(n1, ts.ctx()));
            assert!(!config.exists_with_type<_, _, u8>(n2));
            assert!(!config.exists_with_type_for_next_epoch<_, _, u8>(n2, ts.ctx()));
            assert!(config.read_setting_for_next_epoch(n1) ==  option::some(224u8));
            assert!(config::read_setting<_, u8>(id, n1, ts.ctx()).destroy_some() == 224u8);
            assert!(config::read_setting<_, u8>(id, n2, ts.ctx()).is_none());

            // epoch1
            // n1 -- epoch0 --> 224
            // n1 -- epoch1 --> 0
            config.add_for_next_epoch(&mut WriteCap(), n1, 0u8, ts.ctx());
            assert!(config.exists_with_type<_, _, u8>(n1));
            assert!(config.exists_with_type_for_next_epoch<_, _, u8>(n1, ts.ctx()));
            assert!(!config.exists_with_type<_, _, u8>(n2));
            assert!(!config.exists_with_type_for_next_epoch<_, _, u8>(n2, ts.ctx()));
            assert!(config.borrow_for_next_epoch_mut(&mut WriteCap(), n1, ts.ctx()) == 0u8);
            assert!(config.read_setting_for_next_epoch(n1) ==  option::some(0u8));
            assert!(config::read_setting<_, u8>(id, n1, ts.ctx()).destroy_some() == 224u8);
            assert!(config::read_setting<_, u8>(id, n2, ts.ctx()).is_none());

            // epoch1
            // n1 -- epoch0 --> 224
            // n1 -- epoch1 --> 0
            // n2 -- epoch1 --> 2
            config.add_for_next_epoch(&mut WriteCap(), n2, 2u8, ts.ctx());
            assert!(config.exists_with_type<_, _, u8>(n1));
            assert!(config.exists_with_type_for_next_epoch<_, _, u8>(n1, ts.ctx()));
            assert!(config.exists_with_type<_, _, u8>(n2));
            assert!(config.exists_with_type_for_next_epoch<_, _, u8>(n2, ts.ctx()));
            assert!(config.borrow_for_next_epoch_mut(&mut WriteCap(), n1, ts.ctx()) == 0u8);
            assert!(config.read_setting_for_next_epoch(n1) == option::some(0u8));
            assert!(config.borrow_for_next_epoch_mut(&mut WriteCap(), n2, ts.ctx()) == 2u8);
            assert!(config.read_setting_for_next_epoch(n2) == option::some(2u8));
            assert!(config::read_setting<_, u8>(id, n1, ts.ctx()).destroy_some() == 224u8);
            assert!(config::read_setting<_, u8>(id, n2, ts.ctx()).is_none());
            ts::return_shared(config);
        };

        // check that read_setting is not updated until next epoch
        ts.next_tx(SENDER);
        {
            assert!(config::read_setting<_, u8>(id, n1, ts.ctx()).destroy_some() == 224u8);
            assert!(config::read_setting<_, u8>(id, n2, ts.ctx()).is_none());
        };

        ts.next_epoch(SENDER);
        {
            assert!(config::read_setting<_, u8>(id, n1, ts.ctx()).destroy_some() == 0u8);
            assert!(config::read_setting<_, u8>(id, n2, ts.ctx()).destroy_some() == 2u8);
        };

        ts.next_tx(SENDER);
        {
            let config: Config<WriteCap> = ts.take_shared_by_id(id);
            // epoch2
            // n1 -- epoch0 --> 224
            // n1 -- epoch1 --> 0
            // n2 -- epoch1 --> 2
            assert!(config.exists_with_type<_, _, u8>(n1));
            assert!(!config.exists_with_type_for_next_epoch<_, _, u8>(n1, ts.ctx()));
            assert!(config.exists_with_type<_, _, u8>(n2));
            assert!(!config.exists_with_type_for_next_epoch<_, _, u8>(n2, ts.ctx()));
            assert!(config.read_setting_for_next_epoch(n1) ==  option::some(0u8));
            assert!(config.read_setting_for_next_epoch(n2) ==  option::some(2u8));
            assert!(config::read_setting<_, u8>(id, n1, ts.ctx()).destroy_some() == 0u8);
            assert!(config::read_setting<_, u8>(id, n2, ts.ctx()).destroy_some() == 2u8);
            ts::return_shared(config);
        };

        ts.end();
    }

    #[test, expected_failure(abort_code = sui::config::EAlreadySetForEpoch)]
    fun add_for_next_epoch_aborts_in_same_epoch() {
        let mut ts = ts::begin(SENDER);
        config_create(&mut WriteCap(), ts.ctx());
        ts.next_tx(SENDER);
        let mut config: Config<WriteCap> = ts.take_shared();
        config.add_for_next_epoch(&mut WriteCap(), false, 0u8, ts.ctx());
        config.add_for_next_epoch(&mut WriteCap(), false, 1u8, ts.ctx());
        abort 0
    }

    #[test, expected_failure(abort_code = sui::config::ENotSetForEpoch)]
    fun borrow_for_next_epoch_mut_aborts_in_new_epoch() {
        let mut ts = ts::begin(SENDER);
        config_create(&mut WriteCap(), ts.ctx());
        ts.next_tx(SENDER);
        let mut config: Config<WriteCap> = ts.take_shared();
        let n = 1u64;
        config.add_for_next_epoch(&mut WriteCap(), n, b"hello", ts.ctx());
        assert!(config.exists_with_type<_, _, vector<u8>>(n));
        assert!(config.exists_with_type_for_next_epoch<_, _, vector<u8>>(n, ts.ctx()));
        assert!(config.read_setting_for_next_epoch(n) ==  option::some(b"hello"));

        ts.next_epoch(SENDER);
        assert!(config.exists_with_type<_, _, vector<u8>>(n));
        assert!(!config.exists_with_type_for_next_epoch<_, _, vector<u8>>(n, ts.ctx()));
        assert!(config.read_setting_for_next_epoch(n) ==  option::some(b"hello"));
        // aborts
        config.borrow_for_next_epoch_mut<_, _, vector<u8>>(&mut WriteCap(), n, ts.ctx());
        abort 0
    }

    #[test]
    fun read_setting_none() {
        let mut ts = ts::begin(SENDER);
        config_create(&mut WriteCap(), ts.ctx());
        ts.next_tx(SENDER);
        let id = ts::most_recent_id_shared<Config<WriteCap>>().destroy_some();
        let n = b"hello";
        let w = Wrapped(n);

        // none when not set
        assert!(config::read_setting<_, u8>(id, n, ts.ctx()).is_none());
        assert!(config::read_setting<_, bool>(id, n, ts.ctx()).is_none());
        assert!(config::read_setting<_, u64>(id, n, ts.ctx()).is_none());
        assert!(config::read_setting<_, u8>(id, w, ts.ctx()).is_none());

        ts.next_tx(SENDER);
        {
            let mut config: Config<WriteCap> = ts.take_shared_by_id(id);
            config.add_for_next_epoch(&mut WriteCap(), n, 0u8, ts.ctx());
            ts::return_shared(config);
        };

        // none when the epoch is not advanced
        // but advancing the transaction should populate the cache
        ts.next_tx(SENDER);
        {
            assert!(config::read_setting<_, u8>(id, n, ts.ctx()).is_none());
            assert!(config::read_setting<_, bool>(id, n, ts.ctx()).is_none());
            assert!(config::read_setting<_, u64>(id, n, ts.ctx()).is_none());
            assert!(config::read_setting<_, u8>(id, w, ts.ctx()).is_none());
        };

        // should be readable when the epoch is advanced
        // none for type mismatch
        ts.next_epoch(SENDER);
        {
            // now some
            assert!(config::read_setting<_, u8>(id, n, ts.ctx()).is_some());
            // still none
            assert!(config::read_setting<_, bool>(id, n, ts.ctx()).is_none());
            assert!(config::read_setting<_, u64>(id, n, ts.ctx()).is_none());
            assert!(config::read_setting<_, u8>(id, w, ts.ctx()).is_none());
        };

        // remove the setting, but still readable this epoch
        ts.next_tx(SENDER);
        {
            let mut config: Config<WriteCap> = ts.take_shared_by_id(id);
            config.remove_for_next_epoch<_, _, u8>(&mut WriteCap(), n, ts.ctx());
            // still some
            assert!(config::read_setting<_, u8>(id, n, ts.ctx()).is_some());
            // still none
            assert!(config::read_setting<_, bool>(id, n, ts.ctx()).is_none());
            assert!(config::read_setting<_, u64>(id, n, ts.ctx()).is_none());
            assert!(config::read_setting<_, u8>(id, w, ts.ctx()).is_none());
            ts::return_shared(config);
        };

        // should now be none
        ts.next_epoch(SENDER);
        {
            let mut config: Config<WriteCap> = ts.take_shared_by_id(id);
            assert!(config::read_setting<_, u8>(id, n, ts.ctx()).is_none());
            config.remove_for_next_epoch<_, _, u8>(&mut WriteCap(), n, ts.ctx());
            config.add_for_next_epoch<_, _, u8>(&mut WriteCap(), n, 0, ts.ctx());
            config.remove_for_next_epoch<_, _, u8>(&mut WriteCap(), n, ts.ctx());
            config.add_for_next_epoch<_, _, u8>(&mut WriteCap(), n, 0, ts.ctx());
            config.remove_for_next_epoch<_, _, u8>(&mut WriteCap(), n, ts.ctx());
            // now none
            assert!(config::read_setting<_, u8>(id, n, ts.ctx()).is_none());
            // still none
            assert!(config::read_setting<_, bool>(id, n, ts.ctx()).is_none());
            assert!(config::read_setting<_, u64>(id, n, ts.ctx()).is_none());
            assert!(config::read_setting<_, u8>(id, w, ts.ctx()).is_none());
            ts::return_shared(config);
        };

        // still none
        ts.next_epoch(SENDER);
        {
            let mut config: Config<WriteCap> = ts.take_shared_by_id(id);
            assert!(config::read_setting<_, u8>(id, n, ts.ctx()).is_none());
            config.add_for_next_epoch<_, _, u8>(&mut WriteCap(), n, 0, ts.ctx());
            config.remove_for_next_epoch<_, _, u8>(&mut WriteCap(), n, ts.ctx());
            config.add_for_next_epoch<_, _, u8>(&mut WriteCap(), n, 0, ts.ctx());
            config.remove_for_next_epoch<_, _, u8>(&mut WriteCap(), n, ts.ctx());
            config.add_for_next_epoch<_, _, u8>(&mut WriteCap(), n, 0, ts.ctx());
            config.remove_for_next_epoch<_, _, u8>(&mut WriteCap(), n, ts.ctx());
            // now none
            assert!(config::read_setting<_, u8>(id, n, ts.ctx()).is_none());
            // still none
            assert!(config::read_setting<_, bool>(id, n, ts.ctx()).is_none());
            assert!(config::read_setting<_, u64>(id, n, ts.ctx()).is_none());
            assert!(config::read_setting<_, u8>(id, w, ts.ctx()).is_none());
            ts::return_shared(config);
        };

        ts.end();
    }

    #[test]
    fun test_remove_doesnt_fail_on_duplicate() {
        let mut ts = ts::begin(SENDER);
        config_create(&mut WriteCap(), ts.ctx());
        ts.next_tx(SENDER);
        let id = ts::most_recent_id_shared<Config<WriteCap>>().destroy_some();
        let n = b"hello";
        ts.next_epoch(SENDER);
        {
            let mut config: Config<WriteCap> = ts.take_shared_by_id(id);
            config.remove_for_next_epoch<_, _, u8>(&mut WriteCap(), n, ts.ctx());
            config.remove_for_next_epoch<_, _, u8>(&mut WriteCap(), n, ts.ctx());
            assert!(config::read_setting<_, u8>(id, n, ts.ctx()).is_none());
            ts::return_shared(config);
        };


        ts.next_epoch(SENDER);
        {
            let mut config: Config<WriteCap> = ts.take_shared_by_id(id);
            config.remove_for_next_epoch<_, _, u8>(&mut WriteCap(), n, ts.ctx());
            config.remove_for_next_epoch<_, _, u8>(&mut WriteCap(), n, ts.ctx());
            config.add_for_next_epoch<_, _, u8>(&mut WriteCap(), n, 0, ts.ctx());
            config.remove_for_next_epoch<_, _, u8>(&mut WriteCap(), n, ts.ctx());
            config.add_for_next_epoch<_, _, u8>(&mut WriteCap(), n, 0, ts.ctx());
            config.remove_for_next_epoch<_, _, u8>(&mut WriteCap(), n, ts.ctx());
            config.remove_for_next_epoch<_, _, u8>(&mut WriteCap(), n, ts.ctx());
            assert!(config::read_setting<_, u8>(id, n, ts.ctx()).is_none());
            ts::return_shared(config);
        };

        ts.next_epoch(SENDER);
        {
            let mut config: Config<WriteCap> = ts.take_shared_by_id(id);
            config.add_for_next_epoch<_, _, u8>(&mut WriteCap(), n, 0, ts.ctx());
            assert!(config::read_setting<_, u8>(id, n, ts.ctx()).is_none());
            ts::return_shared(config);
        };

        ts.next_epoch(SENDER);
        {
            let mut config: Config<WriteCap> = ts.take_shared_by_id(id);
            config.add_for_next_epoch<_, _, u8>(&mut WriteCap(), n, 0, ts.ctx());
            assert!(config::read_setting<_, u8>(id, n, ts.ctx()).is_some());
            ts::return_shared(config);
        };

        ts.next_epoch(SENDER);
        {
            let mut config: Config<WriteCap> = ts.take_shared_by_id(id);
            config.remove_for_next_epoch<_, _, u8>(&mut WriteCap(), n, ts.ctx());
            config.remove_for_next_epoch<_, _, u8>(&mut WriteCap(), n, ts.ctx());
            assert!(config::read_setting<_, u8>(id, n, ts.ctx()).is_some());
            ts::return_shared(config);
        };

        ts.next_epoch(SENDER);
        {
            let mut config: Config<WriteCap> = ts.take_shared_by_id(id);
            config.remove_for_next_epoch<_, _, u8>(&mut WriteCap(), n, ts.ctx());
            config.remove_for_next_epoch<_, _, u8>(&mut WriteCap(), n, ts.ctx());
            assert!(config::read_setting<_, u8>(id, n, ts.ctx()).is_none());
            ts::return_shared(config);
        };

        ts.next_epoch(SENDER);
        {
            let mut config: Config<WriteCap> = ts.take_shared_by_id(id);
            config.remove_for_next_epoch<_, _, u8>(&mut WriteCap(), n, ts.ctx());
            config.remove_for_next_epoch<_, _, u8>(&mut WriteCap(), n, ts.ctx());
            assert!(config::read_setting<_, u8>(id, n, ts.ctx()).is_none());
            ts::return_shared(config);
        };

        ts.end();
    }

    #[test, expected_failure(abort_code = sui::dynamic_field::EFieldTypeMismatch)]
    fun test_remove_fail_on_type_mismatch() {
        let mut ts = ts::begin(SENDER);
        config_create(&mut WriteCap(), ts.ctx());
        ts.next_tx(SENDER);
        let id = ts::most_recent_id_shared<Config<WriteCap>>().destroy_some();
        let n = b"hello";
        ts.next_epoch(SENDER);
        let mut config: Config<WriteCap> = ts.take_shared_by_id(id);
        config.add_for_next_epoch<_, _, u8>(&mut WriteCap(), n, 0, ts.ctx());
        config.remove_for_next_epoch<_, _, bool>(&mut WriteCap(), n, ts.ctx());
        abort 0
    }

    #[test, expected_failure(abort_code = sui::dynamic_field::EFieldTypeMismatch)]
    fun test_add_fail_on_type_mismatch() {
        let mut ts = ts::begin(SENDER);
        config_create(&mut WriteCap(), ts.ctx());
        ts.next_tx(SENDER);
        let id = ts::most_recent_id_shared<Config<WriteCap>>().destroy_some();
        let n = b"hello";
        ts.next_epoch(SENDER);
        {
            let mut config: Config<WriteCap> = ts.take_shared_by_id(id);
            config.add_for_next_epoch<_, _, u8>(&mut WriteCap(), n, 0, ts.ctx());
            ts.next_epoch(SENDER);
            ts::return_shared(config);
        };

        ts.next_epoch(SENDER);
        {
            let mut config: Config<WriteCap> = ts.take_shared_by_id(id);
            config.remove_for_next_epoch<_, _, bool>(&mut WriteCap(), n, ts.ctx());
            abort 0
        }
    }

    #[test]
    fun test_removed_value() {
        let mut ts = ts::begin(SENDER);
        config_create(&mut WriteCap(), ts.ctx());
        ts.next_tx(SENDER);
        let id = ts::most_recent_id_shared<Config<WriteCap>>().destroy_some();
        let n = b"hello";
        ts.next_epoch(SENDER);
        {
            let mut config: Config<WriteCap> = ts.take_shared_by_id(id);
            let removed_value =
                config.add_for_next_epoch<_, _, u8>(&mut WriteCap(), n, 0, ts.ctx());
            assert_eq!(removed_value, option::none());
            ts::return_shared(config);
        };

        ts.next_epoch(SENDER);
        {
            let mut config: Config<WriteCap> = ts.take_shared_by_id(id);
            let removed_value =
                config.add_for_next_epoch<_, _, u8>(&mut WriteCap(), n, 1, ts.ctx());
            assert_eq!(removed_value, option::none());
            ts::return_shared(config);
        };

        ts.next_epoch(SENDER);
        {
            let mut config: Config<WriteCap> = ts.take_shared_by_id(id);
            let removed_value =
                config.add_for_next_epoch<_, _, u8>(&mut WriteCap(), n, 2, ts.ctx());
            assert_eq!(removed_value, option::some(0));
            ts::return_shared(config);
        };

        ts.next_epoch(SENDER);
        {
            let mut config: Config<WriteCap> = ts.take_shared_by_id(id);
            let removed_value =
                config.remove_for_next_epoch<_, _, u8>(&mut WriteCap(), n, ts.ctx());
            assert_eq!(removed_value, option::none());
            let removed_value =
                config.add_for_next_epoch<_, _, u8>(&mut WriteCap(), n, 3, ts.ctx());
            assert_eq!(removed_value, option::none());
            ts::return_shared(config);
        };


        ts.next_epoch(SENDER);
        {
            let mut config: Config<WriteCap> = ts.take_shared_by_id(id);
            let removed_value =
                config.add_for_next_epoch<_, _, u8>(&mut WriteCap(), n, 4, ts.ctx());
            assert_eq!(removed_value, option::some(2));
            let removed_value =
                config.remove_for_next_epoch<_, _, u8>(&mut WriteCap(), n, ts.ctx());
            assert_eq!(removed_value, option::some(4));
            let removed_value =
                config.remove_for_next_epoch<_, _, u8>(&mut WriteCap(), n, ts.ctx());
            assert_eq!(removed_value, option::none());
            ts::return_shared(config);
        };

        ts.end();
    }

    // tests
    #[test]
    fun add_remove_cache() {
        let mut ts = ts::begin(SENDER);
        config_create(&mut WriteCap(), ts.ctx());
        ts.next_tx(SENDER);

        let id = ts::most_recent_id_shared<Config<WriteCap>>().destroy_some();
        let n = b"hello";

        ts.next_tx(SENDER);
        {
            let mut config: Config<WriteCap> = ts.take_shared_by_id(id);
            config.add_for_next_epoch<_, _, u8>(&mut WriteCap(), n, 0, ts.ctx());
            ts::return_shared(config);
        };

        ts.next_tx(SENDER);
        {
            let mut config: Config<WriteCap> = ts.take_shared_by_id(id);
            let removed_value =
                config.remove_for_next_epoch<_, _, u8>(&mut WriteCap(), n, ts.ctx());
            assert_eq!(removed_value, option::some(0));
            ts::return_shared(config);
        };

        ts.next_epoch(SENDER);
        {
            let config: Config<WriteCap> = ts.take_shared_by_id(id);
            assert!(config.read_setting_for_next_epoch<_, _, u8>(n).is_none());
            assert!(config::read_setting<_, u8>(id, n, ts.ctx()).is_none());
            ts::return_shared(config);
        };

        ts.end();
    }


}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only, allow(deprecated_usage)]
module sui::coin_tests {
    use sui::coin::{Self, Coin};
    use sui::pay;
    use sui::url;
    use sui::test_scenario;
    use sui::deny_list;

    public struct COIN_TESTS has drop {}

    const TEST_ADDR: address = @0xA11CE;

    fun assert_status(
        deny_list: &deny_list::DenyList,
        addr: address,
        contains_current_epoch: bool,
        contains_next_epoch: bool,
        ctx: &TxContext,
    ) {
        use sui::coin::{
            deny_list_v2_contains_next_epoch as contains_next_epoch,
            deny_list_v2_contains_current_epoch as contains_current_epoch,
        };
        assert!(contains_current_epoch<COIN_TESTS>(deny_list, addr, ctx) == contains_current_epoch);
        assert!(contains_next_epoch<COIN_TESTS>(deny_list, addr) == contains_next_epoch);
    }

    fun assert_global(
        deny_list: &deny_list::DenyList,
        paused_current_epoch: bool,
        paused_next_epoch: bool,
        ctx: &TxContext,
    ) {
        use sui::coin::{
            deny_list_v2_is_global_pause_enabled_next_epoch as is_global_pause_enabled_next_epoch,
            deny_list_v2_is_global_pause_enabled_current_epoch
                as is_global_pause_enabled_current_epoch,
        };
        assert!(
            is_global_pause_enabled_current_epoch<COIN_TESTS>(deny_list, ctx) ==
            paused_current_epoch
        );
        assert!(
            is_global_pause_enabled_next_epoch<COIN_TESTS>(deny_list) == paused_next_epoch
        );
    }

    #[test]
    fun coin_tests_metadata() {
        let mut scenario = test_scenario::begin(TEST_ADDR);
        let ctx = scenario.ctx();
        let witness = COIN_TESTS{};
        let (treasury, mut metadata) = coin::create_currency(
		witness,
		6,
		b"COIN_TESTS",
		b"coin_name",
		b"description",
		option::some(url::new_unsafe_from_bytes(b"icon_url")),
		ctx
	);

        let decimals = metadata.get_decimals();
        let symbol_bytes = metadata.get_symbol<COIN_TESTS>().as_bytes();
        let name_bytes = metadata.get_name<COIN_TESTS>().as_bytes();
        let description_bytes = metadata.get_description<COIN_TESTS>().as_bytes();
        let icon_url = url::inner_url(metadata.get_icon_url<COIN_TESTS>().borrow()).as_bytes();

        assert!(decimals == 6);
        assert!(*symbol_bytes == b"COIN_TESTS");
        assert!(*name_bytes == b"coin_name");
        assert!(*description_bytes == b"description");
        assert!(*icon_url == b"icon_url");

        // Update
        treasury.update_symbol<COIN_TESTS>(&mut metadata, b"NEW_COIN_TESTS".to_ascii_string());
        treasury.update_name<COIN_TESTS>(&mut metadata, b"new_coin_name".to_string());
        treasury.update_description<COIN_TESTS>(&mut metadata, b"new_description".to_string());
        treasury.update_icon_url<COIN_TESTS>(&mut metadata, b"new_icon_url".to_ascii_string());

        let symbol_bytes = metadata.get_symbol<COIN_TESTS>().as_bytes();
        let name_bytes = metadata.get_name<COIN_TESTS>().as_bytes();
        let description_bytes = metadata.get_description<COIN_TESTS>().as_bytes();
        let icon_url = url::inner_url(metadata.get_icon_url<COIN_TESTS>().borrow()).as_bytes();

        assert!(*symbol_bytes == b"NEW_COIN_TESTS");
        assert!(*name_bytes == b"new_coin_name");
        assert!(*description_bytes == b"new_description");
        assert!(*icon_url == b"new_icon_url");

        transfer::public_freeze_object(metadata);
        transfer::public_transfer(treasury, ctx.sender());
        scenario.end();
    }

    #[test]
    fun coin_tests_mint() {
        let mut scenario = test_scenario::begin(TEST_ADDR);
        let witness = COIN_TESTS{};
        let (mut treasury, metadata) = coin::create_currency(
		witness,
		6,
		b"COIN_TESTS",
		b"coin_name",
		b"description",
		option::some(url::new_unsafe_from_bytes(b"icon_url")),
		scenario.ctx()
	);

        let balance = treasury.mint_balance<COIN_TESTS>(1000);
        let coin = coin::from_balance(balance, scenario.ctx());
        let value = coin.value();
        assert!(value == 1000);
        pay::keep(coin, scenario.ctx());

        coin::mint_and_transfer<COIN_TESTS>(&mut treasury, 42, TEST_ADDR, scenario.ctx());
        scenario.next_epoch(TEST_ADDR); // needed or else we won't have a value for `most_recent_id_for_address` coming up next.
        let coin = scenario.take_from_address<Coin<COIN_TESTS>>(TEST_ADDR);
        let value = coin.value();
        assert!(value == 42);
        pay::keep(coin, scenario.ctx());

        transfer::public_freeze_object(metadata);
        transfer::public_transfer(treasury, scenario.ctx().sender());
        scenario.end();
    }

    #[test]
    fun deny_list_v1() {
        let mut scenario = test_scenario::begin(@0);
        deny_list::create_for_test(scenario.ctx());
        scenario.next_tx(TEST_ADDR);

        let witness = COIN_TESTS {};
        let (treasury, mut deny_cap, metadata) = coin::create_regulated_currency(
            witness,
            6,
            b"COIN_TESTS",
            b"coin_name",
            b"description",
            option::some(url::new_unsafe_from_bytes(b"icon_url")),
            scenario.ctx(),
        );
        transfer::public_freeze_object(metadata);
        transfer::public_freeze_object(treasury);
        {
            // test freezing an address
            scenario.next_tx(TEST_ADDR);
            let mut deny_list: deny_list::DenyList = scenario.take_shared();
            assert!(!coin::deny_list_contains<COIN_TESTS>(&deny_list, @100));
            coin::deny_list_add(&mut deny_list, &mut deny_cap, @100, scenario.ctx());
            assert!(coin::deny_list_contains<COIN_TESTS>(&deny_list, @100));
            coin::deny_list_remove(&mut deny_list, &mut deny_cap, @100, scenario.ctx());
            assert!(!coin::deny_list_contains<COIN_TESTS>(&deny_list, @100));
            test_scenario::return_shared(deny_list);
        };
        {
            // test freezing an address over multiple "transactions"
            scenario.next_tx(TEST_ADDR);
            let mut deny_list: deny_list::DenyList = scenario.take_shared();
            assert!(!coin::deny_list_contains<COIN_TESTS>(&deny_list, @100));
            assert!(!coin::deny_list_contains<COIN_TESTS>(&deny_list, @200));
            coin::deny_list_add(&mut deny_list, &mut deny_cap, @200, scenario.ctx());
            assert!(coin::deny_list_contains<COIN_TESTS>(&deny_list, @200));
            test_scenario::return_shared(deny_list);

            scenario.next_tx(TEST_ADDR);
            let mut deny_list: deny_list::DenyList = scenario.take_shared();
            assert!(coin::deny_list_contains<COIN_TESTS>(&deny_list, @200));
            coin::deny_list_remove(&mut deny_list, &mut deny_cap, @200, scenario.ctx());
            assert!(!coin::deny_list_contains<COIN_TESTS>(&deny_list, @200));
            test_scenario::return_shared(deny_list);
        };
        transfer::public_freeze_object(deny_cap);
        scenario.end();
    }

    #[test]
    fun deny_list_v1_double_add() {
        let mut scenario = test_scenario::begin(@0);
        deny_list::create_for_test(scenario.ctx());
        scenario.next_tx(TEST_ADDR);

        let witness = COIN_TESTS {};
        let (treasury, mut deny_cap, metadata) = coin::create_regulated_currency(
            witness,
            6,
            b"COIN_TESTS",
            b"coin_name",
            b"description",
            option::some(url::new_unsafe_from_bytes(b"icon_url")),
            scenario.ctx(),
        );
        transfer::public_freeze_object(metadata);
        transfer::public_freeze_object(treasury);
        {
            // test freezing an address
            scenario.next_tx(TEST_ADDR);
            let mut deny_list: deny_list::DenyList = scenario.take_shared();
            assert!(!coin::deny_list_contains<COIN_TESTS>(&deny_list, @100));
            coin::deny_list_add(&mut deny_list, &mut deny_cap, @100, scenario.ctx());
            coin::deny_list_add(&mut deny_list, &mut deny_cap, @100, scenario.ctx());
            assert!(coin::deny_list_contains<COIN_TESTS>(&deny_list, @100));
            coin::deny_list_remove(&mut deny_list, &mut deny_cap, @100, scenario.ctx());
            assert!(!coin::deny_list_contains<COIN_TESTS>(&deny_list, @100));
            test_scenario::return_shared(deny_list);
        };
        transfer::public_freeze_object(deny_cap);
        scenario.end();
    }

    #[test]
    fun deny_list_v2() {
        use sui::coin::{
            deny_list_v2_add as add,
            deny_list_v2_remove as remove,
        };
        let mut scenario = test_scenario::begin(@0);
        deny_list::create_for_test(scenario.ctx());
        scenario.next_tx(TEST_ADDR);

        let witness = COIN_TESTS {};
        let (treasury, mut deny_cap, metadata) = coin::create_regulated_currency_v2(
            witness,
            6,
            b"COIN_TESTS",
            b"coin_name",
            b"description",
            option::some(url::new_unsafe_from_bytes(b"icon_url")),
            /* allow_global_pause */ true,
            scenario.ctx(),
        );
        transfer::public_freeze_object(metadata);
        transfer::public_freeze_object(treasury);
        scenario.next_epoch(TEST_ADDR);
        {
            // test freezing an address
            let mut deny_list: deny_list::DenyList = scenario.take_shared();
            let ctx = scenario.ctx();
            assert_status(&deny_list, @100, /* current */ false, /* next */ false, ctx);
            add(&mut deny_list, &mut deny_cap, @100, ctx);
            assert_status(&deny_list, @100, /* current */ false, /* next */ true, ctx);
            remove(&mut deny_list, &mut deny_cap, @100, ctx);
            assert_status(&deny_list, @100, /* current */ false, /* next */ false, ctx);
            add(&mut deny_list, &mut deny_cap, @102, ctx);
            assert_status(&deny_list, @102, /* current */ false, /* next */ true, ctx);
            test_scenario::return_shared(deny_list);
        };

        scenario.next_epoch(TEST_ADDR);
        {
            // test freezing an address over multiple "transactions"
            let mut deny_list: deny_list::DenyList = scenario.take_shared();
            let ctx = scenario.ctx();
            assert_status(&deny_list, @100, /* current */ false, /* next */ false, ctx);
            assert_status(&deny_list, @102, /* current */ true, /* next */ true, ctx);
            assert_status(&deny_list, @200, /* current */ false, /* next */ false, ctx);
            add(&mut deny_list, &mut deny_cap, @200, ctx);
            assert_status(&deny_list, @200, /* current */ false, /* next */ true, ctx);
            remove(&mut deny_list, &mut deny_cap, @102, ctx);
            assert_status(&deny_list, @102, /* current */ true, /* next */ false, ctx);
            test_scenario::return_shared(deny_list);
        };

        scenario.next_tx(TEST_ADDR);
        {
            let mut deny_list: deny_list::DenyList = scenario.take_shared();
            let ctx = scenario.ctx();
            assert_status(&deny_list, @102, /* current */ true, /* next */ false, ctx);
            assert_status(&deny_list, @200, /* current */ false, /* next */ true, ctx);
            remove(&mut deny_list, &mut deny_cap, @200, ctx);
            assert_status(&deny_list, @200, /* current */ false, /* next */ false, ctx);
            test_scenario::return_shared(deny_list);
        };

        scenario.next_epoch(TEST_ADDR);
        {
            let deny_list: deny_list::DenyList = scenario.take_shared();
            let ctx = scenario.ctx();
            assert_status(&deny_list, @102, /* current */ false, /* next */ false, ctx);
            test_scenario::return_shared(deny_list);
        };
        transfer::public_freeze_object(deny_cap);
        scenario.end();
    }

    #[test]
    fun deny_list_v2_global_pause() {
        use sui::coin::{
            deny_list_v2_add as add,
            deny_list_v2_remove as remove,
            deny_list_v2_enable_global_pause as enable_global_pause,
            deny_list_v2_disable_global_pause as disable_global_pause,
        };
        let mut scenario = test_scenario::begin(@0);
        deny_list::create_for_test(scenario.ctx());
        scenario.next_tx(TEST_ADDR);

        let witness = COIN_TESTS {};
        let (treasury, mut deny_cap, metadata) = coin::create_regulated_currency_v2(
            witness,
            6,
            b"COIN_TESTS",
            b"coin_name",
            b"description",
            option::some(url::new_unsafe_from_bytes(b"icon_url")),
            /* allow_global_pause */ true,
            scenario.ctx(),
        );
        transfer::public_freeze_object(metadata);
        transfer::public_freeze_object(treasury);
        scenario.next_epoch(TEST_ADDR);
        {
            // global pause =/=> contains
            let mut deny_list: deny_list::DenyList = scenario.take_shared();
            let ctx = scenario.ctx();
            assert_status(&deny_list, @100, /* current */ false, /* next */ false, ctx);
            assert_global(&deny_list, /* current */ false, /* next */ false, ctx);
            enable_global_pause(&mut deny_list, &mut deny_cap, ctx);
            assert_status(&deny_list, @100, /* current */ false, /* next */ false, ctx);
            assert_global(&deny_list, /* current */ false, /* next */ true, ctx);
            // test double enable
            enable_global_pause(&mut deny_list, &mut deny_cap, ctx);
            assert_global(&deny_list, /* current */ false, /* next */ true, ctx);
            test_scenario::return_shared(deny_list);
        };
        scenario.next_epoch(TEST_ADDR);
        {
            // can still add/remove during global pause
            let mut deny_list: deny_list::DenyList = scenario.take_shared();
            let ctx = scenario.ctx();
            assert_status(&deny_list, @100, /* current */ false, /* next */ false, ctx);
            assert_global(&deny_list, /* current */ true, /* next */ true, ctx);
            add(&mut deny_list, &mut deny_cap, @100, ctx);
            assert_status(&deny_list, @100, /* current */ false, /* next */ true, ctx);
            assert_global(&deny_list, /* current */ true, /* next */ true, ctx);
            remove(&mut deny_list, &mut deny_cap, @100, ctx);
            assert_status(&deny_list, @100, /* current */ false, /* next */ false, ctx);
            assert_global(&deny_list, /* current */ true, /* next */ true, ctx);
            test_scenario::return_shared(deny_list);
        };
        scenario.next_epoch(TEST_ADDR);
        {
            // global pause does not affect contains when disabled
            let mut deny_list: deny_list::DenyList = scenario.take_shared();
            let ctx = scenario.ctx();
            assert_status(&deny_list, @100, /* current */ false, /* next */ false, ctx);
            assert_global(&deny_list, /* current */ true, /* next */ true, ctx);
            add(&mut deny_list, &mut deny_cap, @100, ctx);
            assert_status(&deny_list, @100, /* current */ false, /* next */ true, ctx);
            assert_global(&deny_list, /* current */ true, /* next */ true, ctx);
            disable_global_pause(&mut deny_list, &mut deny_cap, ctx);
            assert_status(&deny_list, @100, /* current */ false, /* next */ true, ctx);
            assert_global(&deny_list, /* current */ true, /* next */ false, ctx);
            // test double disable
            disable_global_pause(&mut deny_list, &mut deny_cap, ctx);
            assert_global(&deny_list, /* current */ true, /* next */ false, ctx);
            test_scenario::return_shared(deny_list);
        };
        transfer::public_freeze_object(deny_cap);
        scenario.end();
    }

    #[test]
    fun deny_list_v2_double_add() {
        use sui::coin::{
            deny_list_v2_add as add,
            deny_list_v2_remove as remove,
        };
        let mut scenario = test_scenario::begin(@0);
        deny_list::create_for_test(scenario.ctx());
        scenario.next_tx(TEST_ADDR);

        let witness = COIN_TESTS {};
        let (treasury, mut deny_cap, metadata) = coin::create_regulated_currency_v2(
            witness,
            6,
            b"COIN_TESTS",
            b"coin_name",
            b"description",
            option::some(url::new_unsafe_from_bytes(b"icon_url")),
            /* allow_global_pause */ true,
            scenario.ctx(),
        );
        transfer::public_freeze_object(metadata);
        transfer::public_freeze_object(treasury);
        scenario.next_tx(TEST_ADDR);
        {
            // test freezing an address
            let mut deny_list: deny_list::DenyList = scenario.take_shared();
            let ctx = scenario.ctx();
            assert_status(&deny_list, @100, /* current */ false, /* next */ false, ctx);
            add(&mut deny_list, &mut deny_cap, @100, ctx);
            add(&mut deny_list, &mut deny_cap, @100, ctx);
            assert_status(&deny_list, @100, /* current */ false, /* next */ true, ctx);
            remove(&mut deny_list, &mut deny_cap, @100, ctx);
            assert_status(&deny_list, @100, /* current */ false, /* next */ false, ctx);
            test_scenario::return_shared(deny_list);
        };
        transfer::public_freeze_object(deny_cap);
        scenario.end();
    }

    #[test, expected_failure(abort_code = sui::coin::EGlobalPauseNotAllowed)]
    fun deny_list_v2_global_pause_not_allowed_enable() {
        let mut scenario = test_scenario::begin(@0);
        deny_list::create_for_test(scenario.ctx());
        scenario.next_tx(TEST_ADDR);

        let witness = COIN_TESTS {};
        let (_treasury, mut deny_cap, _metadata) = coin::create_regulated_currency_v2(
            witness,
            6,
            b"COIN_TESTS",
            b"coin_name",
            b"description",
            option::some(url::new_unsafe_from_bytes(b"icon_url")),
            /* allow_global_pause */ false ,
            scenario.ctx(),
        );
        let mut deny_list: deny_list::DenyList = scenario.take_shared();
        coin::deny_list_v2_enable_global_pause(&mut deny_list, &mut deny_cap, scenario.ctx());
        abort 0
    }

    #[test, expected_failure(abort_code = sui::coin::EGlobalPauseNotAllowed)]
    fun deny_list_v2_global_pause_not_allowed_disable() {
        let mut scenario = test_scenario::begin(@0);
        deny_list::create_for_test(scenario.ctx());
        scenario.next_tx(TEST_ADDR);

        let witness = COIN_TESTS {};
        let (_treasury, mut deny_cap, _metadata) = coin::create_regulated_currency_v2(
            witness,
            6,
            b"COIN_TESTS",
            b"coin_name",
            b"description",
            option::some(url::new_unsafe_from_bytes(b"icon_url")),
            /* allow_global_pause */ false ,
            scenario.ctx(),
        );
        let mut deny_list: deny_list::DenyList = scenario.take_shared();
        coin::deny_list_v2_disable_global_pause(&mut deny_list, &mut deny_cap, scenario.ctx());
        abort 0
    }


    #[test]
    fun migrate_regulated_currency_to_v2() {
        let mut scenario = test_scenario::begin(@0);
        deny_list::create_for_test(scenario.ctx());
        scenario.next_tx(TEST_ADDR);

        let witness = COIN_TESTS {};
        let (treasury, mut deny_cap, metadata) = coin::create_regulated_currency(
            witness,
            6,
            b"COIN_TESTS",
            b"coin_name",
            b"description",
            option::some(url::new_unsafe_from_bytes(b"icon_url")),
            scenario.ctx(),
        );
        let deny_cap_v2;
        transfer::public_freeze_object(metadata);
        transfer::public_freeze_object(treasury);
        scenario.next_tx(TEST_ADDR);
        {
            // test freezing an address
            let mut deny_list: deny_list::DenyList = scenario.take_shared();
            let ctx = scenario.ctx();
            coin::deny_list_add(&mut deny_list, &mut deny_cap, @100, ctx);
            coin::deny_list_add(&mut deny_list, &mut deny_cap, @200, ctx);
            coin::deny_list_add(&mut deny_list, &mut deny_cap, @300, ctx);
            assert_status(&deny_list, @100, /* current */ false, /* next */ false, ctx);
            assert_status(&deny_list, @200, /* current */ false, /* next */ false, ctx);
            assert_status(&deny_list, @300, /* current */ false, /* next */ false, ctx);
            test_scenario::return_shared(deny_list);
        };
        scenario.next_tx(TEST_ADDR);
        {
            let mut deny_list: deny_list::DenyList = scenario.take_shared();
            let ctx = scenario.ctx();
            assert!(coin::deny_list_contains<COIN_TESTS>(&deny_list, @100));
            assert!(coin::deny_list_contains<COIN_TESTS>(&deny_list, @200));
            assert!(coin::deny_list_contains<COIN_TESTS>(&deny_list, @300));
            assert_status(&deny_list, @100, /* current */ false, /* next */ false, ctx);
            assert_status(&deny_list, @200, /* current */ false, /* next */ false, ctx);
            assert_status(&deny_list, @300, /* current */ false, /* next */ false, ctx);
            deny_cap_v2 = coin::migrate_regulated_currency_to_v2(&mut deny_list, deny_cap, true, ctx);
            assert_status(&deny_list, @100, /* current */ false, /* next */ true, ctx);
            assert_status(&deny_list, @200, /* current */ false, /* next */ true, ctx);
            assert_status(&deny_list, @300, /* current */ false, /* next */ true, ctx);
            test_scenario::return_shared(deny_list);
        };
        scenario.next_epoch(TEST_ADDR);
        {
            let deny_list: deny_list::DenyList = scenario.take_shared();
            let ctx = scenario.ctx();
            assert!(!coin::deny_list_contains<COIN_TESTS>(&deny_list, @100));
            assert!(!coin::deny_list_contains<COIN_TESTS>(&deny_list, @200));
            assert!(!coin::deny_list_contains<COIN_TESTS>(&deny_list, @300));
            assert_status(&deny_list, @100, /* current */ true, /* next */ true, ctx);
            assert_status(&deny_list, @200, /* current */ true, /* next */ true, ctx);
            assert_status(&deny_list, @300, /* current */ true, /* next */ true, ctx);
            test_scenario::return_shared(deny_list);
        };
        transfer::public_freeze_object(deny_cap_v2);
        scenario.end();
    }

    #[test, expected_failure(abort_code = sui::coin::EGlobalPauseNotAllowed)]
    fun migrate_regulated_currency_to_v2_disallow_global_pause() {
        let mut scenario = test_scenario::begin(@0);
        deny_list::create_for_test(scenario.ctx());
        scenario.next_tx(TEST_ADDR);

        let witness = COIN_TESTS {};
        let (treasury, mut deny_cap, metadata) = coin::create_regulated_currency(
            witness,
            6,
            b"COIN_TESTS",
            b"coin_name",
            b"description",
            option::some(url::new_unsafe_from_bytes(b"icon_url")),
            scenario.ctx(),
        );
        let mut deny_cap_v2;
        transfer::public_freeze_object(metadata);
        transfer::public_freeze_object(treasury);
        scenario.next_tx(TEST_ADDR);
        {
            // test freezing an address
            let mut deny_list: deny_list::DenyList = scenario.take_shared();
            coin::deny_list_add(&mut deny_list, &mut deny_cap, @100, scenario.ctx());
            test_scenario::return_shared(deny_list);
        };
        scenario.next_tx(TEST_ADDR);
        {
            let mut deny_list: deny_list::DenyList = scenario.take_shared();
            assert!(coin::deny_list_contains<COIN_TESTS>(&deny_list, @100));
            assert_status(&deny_list, @100, /* current */ false, /* next */ false, scenario.ctx());
            deny_cap_v2 = coin::migrate_regulated_currency_to_v2(&mut deny_list, deny_cap, false, scenario.ctx());
            assert!(!coin::deny_list_contains<COIN_TESTS>(&deny_list, @100));
            assert_status(&deny_list, @100, /* current */ false, /* next */ true, scenario.ctx());
            test_scenario::return_shared(deny_list);
        };
        scenario.next_epoch(TEST_ADDR);
        {
            let deny_list: deny_list::DenyList = scenario.take_shared();
            assert!(!coin::deny_list_contains<COIN_TESTS>(&deny_list, @100));
            assert_status(&deny_list, @100, /* current */ true, /* next */ true, scenario.ctx());
            test_scenario::return_shared(deny_list);
        };
        scenario.next_tx(TEST_ADDR);
        let mut deny_list: deny_list::DenyList = scenario.take_shared();
        coin::deny_list_v2_enable_global_pause(&mut deny_list, &mut deny_cap_v2, scenario.ctx());
        abort 0
    }

    #[test]
    fun deny_list_v2_add_remove() {
        use sui::coin::{
            deny_list_v2_add as add,
            deny_list_v2_remove as remove,
        };
        let mut scenario = test_scenario::begin(@0);
        deny_list::create_for_test(scenario.ctx());
        scenario.next_tx(TEST_ADDR);

        let witness = COIN_TESTS {};
        let (treasury, mut deny_cap, metadata) = coin::create_regulated_currency_v2(
            witness,
            6,
            b"COIN_TESTS",
            b"coin_name",
            b"description",
            option::some(url::new_unsafe_from_bytes(b"icon_url")),
            /* allow_global_pause */ true,
            scenario.ctx(),
        );
        transfer::public_freeze_object(metadata);
        transfer::public_freeze_object(treasury);

        scenario.next_epoch(TEST_ADDR);
        {
            let mut deny_list: deny_list::DenyList = scenario.take_shared();
            let ctx = scenario.ctx();
            add(&mut deny_list, &mut deny_cap, @300, ctx);
            add(&mut deny_list, &mut deny_cap, @400, ctx);
            add(&mut deny_list, &mut deny_cap, @500, ctx);
            assert_status(&deny_list, @100, /* current */ false, /* next */ false, ctx);
            assert_status(&deny_list, @200, /* current */ false, /* next */ false, ctx);
            assert_status(&deny_list, @300, /* current */ false, /* next */ true, ctx);
            assert_status(&deny_list, @400, /* current */ false, /* next */ true, ctx);
            assert_status(&deny_list, @500, /* current */ false, /* next */ true, ctx);
            test_scenario::return_shared(deny_list);
        };

        scenario.next_epoch(TEST_ADDR);
        {
            let mut deny_list: deny_list::DenyList = scenario.take_shared();
            let ctx = scenario.ctx();
            add(&mut deny_list, &mut deny_cap, @200, ctx);
            remove(&mut deny_list, &mut deny_cap, @300, ctx);
            remove(&mut deny_list, &mut deny_cap, @400, ctx);
            add(&mut deny_list, &mut deny_cap, @500, ctx);
            remove(&mut deny_list, &mut deny_cap, @500, ctx);
            assert_status(&deny_list, @100, /* current */ false, /* next */ false, ctx);
            assert_status(&deny_list, @200, /* current */ false, /* next */ true, ctx);
            assert_status(&deny_list, @300, /* current */ true, /* next */ false, ctx);
            assert_status(&deny_list, @400, /* current */ true, /* next */ false, ctx);
            assert_status(&deny_list, @500, /* current */ true, /* next */ false, ctx);
            test_scenario::return_shared(deny_list);
        };

        scenario.next_epoch(TEST_ADDR);
        {
            let mut deny_list: deny_list::DenyList = scenario.take_shared();
            let ctx = scenario.ctx();
            add(&mut deny_list, &mut deny_cap, @100, ctx);
            remove(&mut deny_list, &mut deny_cap, @100, ctx);
            remove(&mut deny_list, &mut deny_cap, @200, ctx);
            remove(&mut deny_list, &mut deny_cap, @300, ctx);
            assert_status(&deny_list, @100, /* current */ false, /* next */ false, ctx);
            assert_status(&deny_list, @200, /* current */ true, /* next */ false, ctx);
            assert_status(&deny_list, @300, /* current */ false, /* next */ false, ctx);
            assert_status(&deny_list, @400, /* current */ false, /* next */ false, ctx);
            assert_status(&deny_list, @500, /* current */ false, /* next */ false, ctx);
            test_scenario::return_shared(deny_list);
        };

        scenario.next_epoch(TEST_ADDR);
        {
            let deny_list: deny_list::DenyList = scenario.take_shared();
            let ctx = scenario.ctx();
            assert_status(&deny_list, @100, /* current */ false, /* next */ false, ctx);
            assert_status(&deny_list, @200, /* current */ false, /* next */ false, ctx);
            assert_status(&deny_list, @300, /* current */ false, /* next */ false, ctx);
            assert_status(&deny_list, @400, /* current */ false, /* next */ false, ctx);
            assert_status(&deny_list, @500, /* current */ false, /* next */ false, ctx);
            test_scenario::return_shared(deny_list);
        };
        transfer::public_freeze_object(deny_cap);
        scenario.end();
    }
}

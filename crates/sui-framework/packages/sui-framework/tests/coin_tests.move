// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::coin_tests {
    use std::option;
    use sui::coin::{Self, Coin};
    use sui::pay;
    use sui::url;
    use sui::test_scenario;
    use sui::transfer;
    use sui::tx_context;
    use std::string;
    use std::ascii;
    use sui::deny_list;

    public struct COIN_TESTS has drop {}

    const TEST_ADDR: address = @0xA11CE;

    #[test]
    fun coin_tests_metadata() {
        let mut scenario = test_scenario::begin(TEST_ADDR);
        let test = &mut scenario;
        let ctx = test_scenario::ctx(test);
        let witness = COIN_TESTS{};
        let (treasury, mut metadata) = coin::create_currency(witness, 6, b"COIN_TESTS", b"coin_name", b"description", option::some(url::new_unsafe_from_bytes(b"icon_url")), ctx);

        let decimals = metadata.get_decimals();
        let symbol_bytes = ascii::as_bytes(&metadata.get_symbol<COIN_TESTS>());
        let name_bytes = string::bytes(&metadata.get_name<COIN_TESTS>());
        let description_bytes = string::bytes(&metadata.get_description<COIN_TESTS>());
        let icon_url = ascii::as_bytes(&url::inner_url(option::borrow(&metadata.get_icon_url<COIN_TESTS>())));

        assert!(decimals == 6, 0);
        assert!(*symbol_bytes == b"COIN_TESTS", 0);
        assert!(*name_bytes == b"coin_name", 0);
        assert!(*description_bytes == b"description", 0);
        assert!(*icon_url == b"icon_url", 0);

        // Update
        treasury.update_symbol<COIN_TESTS>(&mut metadata, b"NEW_COIN_TESTS".to_ascii_string());
        treasury.update_name<COIN_TESTS>(&mut metadata, b"new_coin_name".to_string());
        treasury.update_description<COIN_TESTS>(&mut metadata, b"new_description".to_string());
        treasury.update_icon_url<COIN_TESTS>(&mut metadata, b"new_icon_url".to_ascii_string());

        let symbol_bytes = metadata.get_symbol<COIN_TESTS>().as_bytes();
        let name_bytes = metadata.get_name<COIN_TESTS>().bytes();
        let description_bytes = metadata.get_description<COIN_TESTS>().bytes();
        let icon_url = url::inner_url(metadata.get_icon_url<COIN_TESTS>().borrow()).as_bytes();

        assert!(*symbol_bytes == b"NEW_COIN_TESTS", 0);
        assert!(*name_bytes == b"new_coin_name", 0);
        assert!(*description_bytes == b"new_description", 0);
        assert!(*icon_url == b"new_icon_url", 0);

        transfer::public_freeze_object(metadata);
        transfer::public_transfer(treasury, tx_context::sender(ctx));
        test_scenario::end(scenario);
    }

    #[test]
    fun coin_tests_mint() {
        let mut scenario = test_scenario::begin(TEST_ADDR);
        let test = &mut scenario;
        let witness = COIN_TESTS{};
        let (mut treasury, metadata) = coin::create_currency(witness, 6, b"COIN_TESTS", b"coin_name", b"description", option::some(url::new_unsafe_from_bytes(b"icon_url")), test_scenario::ctx(test));

        let balance = coin::mint_balance<COIN_TESTS>(&mut treasury, 1000);
        let coin = coin::from_balance(balance, test_scenario::ctx(test));
        let value = coin.value();
        assert!(value == 1000, 0);
        pay::keep(coin, test_scenario::ctx(test));

        coin::mint_and_transfer<COIN_TESTS>(&mut treasury, 42, TEST_ADDR, test_scenario::ctx(test));
        test_scenario::next_epoch(test, TEST_ADDR); // needed or else we won't have a value for `most_recent_id_for_address` coming up next.
        let coin = test_scenario::take_from_address<Coin<COIN_TESTS>>(test, TEST_ADDR);
        let value = coin.value();
        assert!(value == 42, 0);
        pay::keep(coin, test_scenario::ctx(test));

        transfer::public_freeze_object(metadata);
        transfer::public_transfer(treasury, tx_context::sender(test_scenario::ctx(test)));
        test_scenario::end(scenario);
    }

    #[test]
    fun deny_list() {
        let mut scenario = test_scenario::begin(@0);
        let test = &mut scenario;
        deny_list::create_for_test(test_scenario::ctx(test));
        test_scenario::next_tx(test, TEST_ADDR);

        let witness = COIN_TESTS {};
        let (treasury, mut deny_cap, metadata) = coin::create_regulated_currency(
            witness,
            6,
            b"COIN_TESTS",
            b"coin_name",
            b"description",
            option::some(url::new_unsafe_from_bytes(b"icon_url")),
            test_scenario::ctx(test),
        );
        transfer::public_freeze_object(metadata);
        transfer::public_freeze_object(treasury);
        {
            // test freezing an address
            test_scenario::next_tx(test, TEST_ADDR);
            let mut deny_list: deny_list::DenyList = test_scenario::take_shared(test);
            assert!(!coin::deny_list_contains<COIN_TESTS>(&deny_list, @1), 0);
            coin::deny_list_add(&mut deny_list, &mut deny_cap, @1, test_scenario::ctx(test));
            assert!(coin::deny_list_contains<COIN_TESTS>(&deny_list, @1), 0);
            coin::deny_list_remove(&mut deny_list, &mut deny_cap, @1, test_scenario::ctx(test));
            assert!(!coin::deny_list_contains<COIN_TESTS>(&deny_list, @1), 0);
            test_scenario::return_shared(deny_list);
        };
        {
            // test freezing an address over multiple "transactions"
            test_scenario::next_tx(test, TEST_ADDR);
            let mut deny_list: deny_list::DenyList = test_scenario::take_shared(test);
            assert!(!coin::deny_list_contains<COIN_TESTS>(&deny_list, @1), 0);
            assert!(!coin::deny_list_contains<COIN_TESTS>(&deny_list, @2), 0);
            coin::deny_list_add(&mut deny_list, &mut deny_cap, @2, test_scenario::ctx(test));
            assert!(coin::deny_list_contains<COIN_TESTS>(&deny_list, @2), 0);
            test_scenario::return_shared(deny_list);

            test_scenario::next_tx(test, TEST_ADDR);
            let mut deny_list: deny_list::DenyList = test_scenario::take_shared(test);
            assert!(coin::deny_list_contains<COIN_TESTS>(&deny_list, @2), 0);
            coin::deny_list_remove(&mut deny_list, &mut deny_cap, @2, test_scenario::ctx(test));
            assert!(!coin::deny_list_contains<COIN_TESTS>(&deny_list, @2), 0);
            test_scenario::return_shared(deny_list);
        };
        transfer::public_freeze_object(deny_cap);
        test_scenario::end(scenario);
    }

    #[test]
    fun address_is_frozen_with_arbitrary_types() {

    }
}

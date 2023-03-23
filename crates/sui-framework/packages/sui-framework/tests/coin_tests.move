// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::coin_tests {
    use std::option;
    use sui::coin::{Self, Coin};
    use sui::pay;
    use sui::test_scenario;
    use sui::transfer;
    use sui::tx_context;
    use sui::vec_map;
    use sui::display;
    use std::string::utf8;

    struct COIN_TESTS has drop {}

    const TEST_ADDR: address = @0xA11CE;

    #[test]
    fun coin_tests_metadata() {
        let scenario = test_scenario::begin(TEST_ADDR);
        let test = &mut scenario;
        let ctx = test_scenario::ctx(test);
        let witness = COIN_TESTS{};
        let (treasury, metadata) = coin::create_currency(
            witness,
            6,
            utf8(b"COIN_TESTS"),
            utf8(b"coin_name"),
            utf8(b"description"),
            option::some(utf8(b"icon_url")),
            ctx
        );

        // mutable borrow of inner `Display` object in the `CoinMetadata`
        let display_mut = coin::metadata_display_mut(&treasury, &mut metadata);
        let fields = display::fields_mut(display_mut);

        // decimals are encoded as `base16` string via `sui::hex::encode`
        let decimals = vec_map::get(fields, &utf8(b"decimals"));
        let symbol_bytes = vec_map::get(fields, &utf8(b"symbol"));
        let name_bytes = vec_map::get(fields, &utf8(b"name"));
        let description_bytes = vec_map::get(fields, &utf8(b"description"));
        let icon_url = vec_map::get(fields, &utf8(b"icon_url"));

        assert!(*decimals == utf8(b"06"), 0);
        assert!(*symbol_bytes == utf8(b"COIN_TESTS"), 0);
        assert!(*name_bytes == utf8(b"coin_name"), 0);
        assert!(*description_bytes == utf8(b"description"), 0);
        assert!(*icon_url == utf8(b"icon_url"), 0);

        transfer::public_freeze_object(metadata);
        transfer::public_transfer(treasury, tx_context::sender(ctx));
        test_scenario::end(scenario);
    }

    #[test]
    fun coin_tests_mint() {
        let scenario = test_scenario::begin(TEST_ADDR);
        let test = &mut scenario;
        let witness = COIN_TESTS{};
        let (treasury, metadata) = coin::create_currency(
            witness,
            6,
            utf8(b"COIN_TESTS"),
            utf8(b"coin_name"),
            utf8(b"description"),
            option::some(utf8(b"icon_url")),
            test_scenario::ctx(test)
        );

        let balance = coin::mint_balance<COIN_TESTS>(&mut treasury, 1000);
        let coin = coin::from_balance(balance, test_scenario::ctx(test));
        let value = coin::value(&coin);
        assert!(value == 1000, 0);
        pay::keep(coin, test_scenario::ctx(test));

        coin::mint_and_transfer<COIN_TESTS>(&mut treasury, 42, TEST_ADDR, test_scenario::ctx(test));
        test_scenario::next_epoch(test, TEST_ADDR); // needed or else we won't have a value for `most_recent_id_for_address` coming up next.
        let coin = test_scenario::take_from_address<Coin<COIN_TESTS>>(test, TEST_ADDR);
        let value = coin::value(&coin);
        assert!(value == 42, 0);
        pay::keep(coin, test_scenario::ctx(test));

        transfer::public_freeze_object(metadata);
        transfer::public_transfer(treasury, tx_context::sender(test_scenario::ctx(test)));
        test_scenario::end(scenario);
    }
}

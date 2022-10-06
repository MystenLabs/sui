// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::wallet_tests {
    use std::vector;
    use sui::test_scenario::{Self};
    use sui::coin;
    use sui::wallet;
    use sui::sui::SUI;
    use sui::coin::Coin;

    const TEST_SENDER_ADDR: address = @0xA11CE;

    #[test]
    public entry fun test_coin_split_n() {
        let scenario = &mut test_scenario::begin(&TEST_SENDER_ADDR);
        let ctx = test_scenario::ctx(scenario);
        let coin = coin::mint_for_testing<SUI>(10, ctx);

        test_scenario::next_tx(scenario, &TEST_SENDER_ADDR);
        wallet::split_n(&mut coin, 3, test_scenario::ctx(scenario));

        test_scenario::next_tx(scenario, &TEST_SENDER_ADDR);
        let coin1 = test_scenario::take_last_created_owned<Coin<SUI>>(scenario);

        test_scenario::next_tx(scenario, &TEST_SENDER_ADDR);
        let coin2 = test_scenario::take_last_created_owned<Coin<SUI>>(scenario);

        test_scenario::next_tx(scenario, &TEST_SENDER_ADDR);
        assert!(coin::value(&coin1) == 3, 0);
        assert!(coin::value(&coin2) == 3, 0);
        assert!(coin::value(&coin) == 4, 0);
        assert!(test_scenario::can_take_owned<Coin<SUI>>(scenario) == false, 1);

        coin::destroy_for_testing(coin);
        coin::destroy_for_testing(coin1);
        coin::destroy_for_testing(coin2);
    }

    #[test]
    public entry fun test_coin_split_n_to_vec() {
        let scenario = &mut test_scenario::begin(&TEST_SENDER_ADDR);
        let ctx = test_scenario::ctx(scenario);
        let coin = coin::mint_for_testing<SUI>(10, ctx);

        test_scenario::next_tx(scenario, &TEST_SENDER_ADDR);
        let split_coins = wallet::split_n_to_vec(&mut coin, 3, test_scenario::ctx(scenario));

        assert!(vector::length(&split_coins) == 2, 0);
        let coin1 = vector::pop_back(&mut split_coins);
        let coin2 = vector::pop_back(&mut split_coins);
        assert!(coin::value(&coin1) == 3, 0);
        assert!(coin::value(&coin2) == 3, 0);
        assert!(coin::value(&coin) == 4, 0);

        vector::destroy_empty(split_coins);
        coin::destroy_for_testing(coin);
        coin::destroy_for_testing(coin1);
        coin::destroy_for_testing(coin2);
    }
}

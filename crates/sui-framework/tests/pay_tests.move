// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::pay_tests {
    use std::vector;
    use sui::test_scenario::{Self};
    use sui::coin::{Self, Coin};
    use sui::pay;
    use sui::sui::SUI;

    const TEST_SENDER_ADDR: address = @0xA11CE;

    #[test]
    public entry fun test_coin_split_n() {
        let scenario_val = test_scenario::begin(TEST_SENDER_ADDR);
        let scenario = &mut scenario_val;
        let ctx = test_scenario::ctx(scenario);
        let coin = coin::mint_for_testing<SUI>(10, ctx);

        test_scenario::next_tx(scenario, TEST_SENDER_ADDR);
        pay::divide_and_keep(&mut coin, 3, test_scenario::ctx(scenario));

        test_scenario::next_tx(scenario, TEST_SENDER_ADDR);
        let coin1 = test_scenario::take_from_sender<Coin<SUI>>(scenario);

        test_scenario::next_tx(scenario, TEST_SENDER_ADDR);
        let coin2 = test_scenario::take_from_sender<Coin<SUI>>(scenario);

        test_scenario::next_tx(scenario, TEST_SENDER_ADDR);
        assert!(coin::value(&coin1) == 3, 0);
        assert!(coin::value(&coin2) == 3, 0);
        assert!(coin::value(&coin) == 4, 0);
        assert!(
            !test_scenario::has_most_recent_for_sender<Coin<SUI>>(scenario),
            1
        );

        coin::destroy_for_testing(coin);
        coin::destroy_for_testing(coin1);
        coin::destroy_for_testing(coin2);
        test_scenario::end(scenario_val);
    }

    #[test]
    public entry fun test_coin_split_n_to_vec() {
        let scenario_val = test_scenario::begin(TEST_SENDER_ADDR);
        let scenario = &mut scenario_val;
        let ctx = test_scenario::ctx(scenario);
        let coin = coin::mint_for_testing<SUI>(10, ctx);

        test_scenario::next_tx(scenario, TEST_SENDER_ADDR);
        let split_coins = coin::divide_into_n(&mut coin, 3, test_scenario::ctx(scenario));

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
        test_scenario::end(scenario_val);
    }
}

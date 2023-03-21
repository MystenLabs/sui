// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::pay_tests {
    use std::vector;
    use sui::test_scenario::{Self};
    use sui::coin::{Self, Coin};
    use sui::pay;
    use sui::balance;
    use sui::sui::SUI;
    use sui::test_utils;

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

        test_utils::destroy(coin);
        test_utils::destroy(coin1);
        test_utils::destroy(coin2);
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
        test_utils::destroy(coin);
        test_utils::destroy(coin1);
        test_utils::destroy(coin2);
        test_scenario::end(scenario_val);
    }

    #[test]
    public entry fun test_split_vec() {
        let scenario_val = test_scenario::begin(TEST_SENDER_ADDR);
        let scenario = &mut scenario_val;
        let ctx = test_scenario::ctx(scenario);
        let coin = coin::mint_for_testing<SUI>(10, ctx);

        test_scenario::next_tx(scenario, TEST_SENDER_ADDR);
        let v = vector[1, 4];
        pay::split_vec(&mut coin, v, test_scenario::ctx(scenario));

        test_scenario::next_tx(scenario, TEST_SENDER_ADDR);
        let coin1 = test_scenario::take_from_sender<Coin<SUI>>(scenario);

        test_scenario::next_tx(scenario, TEST_SENDER_ADDR);
        let coin2 = test_scenario::take_from_sender<Coin<SUI>>(scenario);

        assert!(coin::value(&coin1) == 4, 0);
        assert!(coin::value(&coin2) == 1, 0);
        assert!(coin::value(&coin) == 5, 0);

        test_utils::destroy(coin);
        test_utils::destroy(coin1);
        test_utils::destroy(coin2);
        test_scenario::end(scenario_val);
    }

    #[test]
    public entry fun test_split_and_transfer() {
        let scenario_val = test_scenario::begin(TEST_SENDER_ADDR);
        let scenario = &mut scenario_val;
        let ctx = test_scenario::ctx(scenario);
        let coin = coin::mint_for_testing<SUI>(10, ctx);

        test_scenario::next_tx(scenario, TEST_SENDER_ADDR);
        // Send 3 of 10
        pay::split_and_transfer(&mut coin, 3, TEST_SENDER_ADDR, test_scenario::ctx(scenario));

        test_scenario::next_tx(scenario, TEST_SENDER_ADDR);
        let coin1 = test_scenario::take_from_sender<Coin<SUI>>(scenario);
        assert!(coin::value(&coin1) == 3, 0);
        assert!(coin::value(&coin) == 7, 0);

        test_utils::destroy(coin);
        test_utils::destroy(coin1);
        test_scenario::end(scenario_val);
    }

    #[test]
    #[expected_failure(abort_code = balance::ENotEnough)]
    public entry fun test_split_and_transfer_fail() {
        let scenario_val = test_scenario::begin(TEST_SENDER_ADDR);
        let scenario = &mut scenario_val;
        let ctx = test_scenario::ctx(scenario);
        let coin = coin::mint_for_testing<SUI>(10, ctx);

        test_scenario::next_tx(scenario, TEST_SENDER_ADDR);
        // Send 20 of 10 (should fail)
        pay::split_and_transfer(&mut coin, 20, TEST_SENDER_ADDR, test_scenario::ctx(scenario));
        test_scenario::next_tx(scenario, TEST_SENDER_ADDR);
        let coin_transfer_fail = test_scenario::take_from_sender<Coin<SUI>>(scenario);
        assert!(coin::value(&coin_transfer_fail) == 7, 0);

        test_utils::destroy(coin);
        test_utils::destroy(coin_transfer_fail);
        test_scenario::end(scenario_val);
    }

    #[test]
    public entry fun test_join_vec_and_transfer() {
        let scenario_val = test_scenario::begin(TEST_SENDER_ADDR);
        let scenario = &mut scenario_val;
        let ctx = test_scenario::ctx(scenario);
        let coin = coin::mint_for_testing<SUI>(10, ctx);

        test_scenario::next_tx(scenario, TEST_SENDER_ADDR);
        // divide_into_n with `n = 4` creates a vector of `n-1` = `3` coins containing balance `2`
        let coin_vector = coin::divide_into_n(&mut coin, 4, test_scenario::ctx(scenario));
        pay::join_vec_and_transfer(coin_vector, TEST_SENDER_ADDR);

        test_scenario::next_tx(scenario, TEST_SENDER_ADDR);
        let coin1 = test_scenario::take_from_sender<Coin<SUI>>(scenario);

        // result is `3` coins of balance `2`
        assert!(coin::value(&coin1) == 6, 0);
        assert!(coin::value(&coin) == 4, 0);

        test_utils::destroy(coin);
        test_utils::destroy(coin1);
        test_scenario::end(scenario_val);
    }
}

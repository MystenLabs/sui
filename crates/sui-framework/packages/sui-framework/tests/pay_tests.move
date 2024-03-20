// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::pay_tests {
    use std::vector;
    use sui::test_scenario;
    use sui::coin::{Self, Coin};
    use sui::pay;
    use sui::balance;
    use sui::sui::SUI;
    use sui::test_utils;

    const TEST_SENDER_ADDR: address = @0xA11CE;

    #[test]
    fun test_coin_split_n() {
        let mut scenario_val = test_scenario::begin(TEST_SENDER_ADDR);
        let scenario = &mut scenario_val;
        let ctx = scenario.ctx();
        let mut coin = coin::mint_for_testing<SUI>(10, ctx);

        scenario.next_tx(TEST_SENDER_ADDR);
        pay::divide_and_keep(&mut coin, 3, scenario.ctx());

        scenario.next_tx(TEST_SENDER_ADDR);
        let coin1 = scenario.take_from_sender<Coin<SUI>>();

        scenario.next_tx(TEST_SENDER_ADDR);
        let coin2 = scenario.take_from_sender<Coin<SUI>>();

        scenario.next_tx(TEST_SENDER_ADDR);
        assert!(coin1.value() == 3, 0);
        assert!(coin2.value() == 3, 0);
        assert!(coin.value() == 4, 0);
        assert!(
            !scenario.has_most_recent_for_sender<Coin<SUI>>(),
            1
        );

        test_utils::destroy(coin);
        test_utils::destroy(coin1);
        test_utils::destroy(coin2);
        scenario_val.end();
    }

    #[test]
    fun test_coin_split_n_to_vec() {
        let mut scenario_val = test_scenario::begin(TEST_SENDER_ADDR);
        let scenario = &mut scenario_val;
        let ctx = scenario.ctx();
        let mut coin = coin::mint_for_testing<SUI>(10, ctx);

        scenario.next_tx(TEST_SENDER_ADDR);
        let mut split_coins = coin.divide_into_n(3, scenario.ctx());

        assert!(vector::length(&split_coins) == 2, 0);
        let coin1 = vector::pop_back(&mut split_coins);
        let coin2 = vector::pop_back(&mut split_coins);
        assert!(coin1.value() == 3, 0);
        assert!(coin2.value() == 3, 0);
        assert!(coin.value() == 4, 0);

        vector::destroy_empty(split_coins);
        test_utils::destroy(coin);
        test_utils::destroy(coin1);
        test_utils::destroy(coin2);
        scenario_val.end();
    }

    #[test]
    fun test_split_vec() {
        let mut scenario_val = test_scenario::begin(TEST_SENDER_ADDR);
        let scenario = &mut scenario_val;
        let ctx = scenario.ctx();
        let mut coin = coin::mint_for_testing<SUI>(10, ctx);

        scenario.next_tx(TEST_SENDER_ADDR);
        let v = vector[1, 4];
        pay::split_vec(&mut coin, v, scenario.ctx());

        scenario.next_tx(TEST_SENDER_ADDR);
        let coin1 = scenario.take_from_sender<Coin<SUI>>();

        scenario.next_tx(TEST_SENDER_ADDR);
        let coin2 = scenario.take_from_sender<Coin<SUI>>();

        assert!(coin1.value() == 4, 0);
        assert!(coin2.value() == 1, 0);
        assert!(coin.value() == 5, 0);

        test_utils::destroy(coin);
        test_utils::destroy(coin1);
        test_utils::destroy(coin2);
        scenario_val.end();
    }

    #[test]
    fun test_split_and_transfer() {
        let mut scenario_val = test_scenario::begin(TEST_SENDER_ADDR);
        let scenario = &mut scenario_val;
        let ctx = scenario.ctx();
        let mut coin = coin::mint_for_testing<SUI>(10, ctx);

        scenario.next_tx(TEST_SENDER_ADDR);
        // Send 3 of 10
        pay::split_and_transfer(&mut coin, 3, TEST_SENDER_ADDR, scenario.ctx());

        scenario.next_tx(TEST_SENDER_ADDR);
        let coin1 = scenario.take_from_sender<Coin<SUI>>();
        assert!(coin1.value() == 3, 0);
        assert!(coin.value() == 7, 0);

        test_utils::destroy(coin);
        test_utils::destroy(coin1);
        scenario_val.end();
    }

    #[test]
    #[expected_failure(abort_code = balance::ENotEnough)]
    fun test_split_and_transfer_fail() {
        let mut scenario_val = test_scenario::begin(TEST_SENDER_ADDR);
        let scenario = &mut scenario_val;
        let ctx = scenario.ctx();
        let mut coin = coin::mint_for_testing<SUI>(10, ctx);

        scenario.next_tx(TEST_SENDER_ADDR);
        // Send 20 of 10 (should fail)
        pay::split_and_transfer(&mut coin, 20, TEST_SENDER_ADDR, scenario.ctx());
        scenario.next_tx(TEST_SENDER_ADDR);
        let coin_transfer_fail = scenario.take_from_sender<Coin<SUI>>();
        assert!(&coin_transfer_fail.value() == 7, 0);

        test_utils::destroy(coin);
        test_utils::destroy(coin_transfer_fail);
        scenario_val.end();
    }

    #[test]
    fun test_join_vec_and_transfer() {
        let mut scenario_val = test_scenario::begin(TEST_SENDER_ADDR);
        let scenario = &mut scenario_val;
        let ctx = scenario.ctx();
        let mut coin = coin::mint_for_testing<SUI>(10, ctx);

        scenario.next_tx(TEST_SENDER_ADDR);
        // divide_into_n with `n = 4` creates a vector of `n-1` = `3` coins containing balance `2`
        let coin_vector = coin.divide_into_n(4, scenario.ctx());
        pay::join_vec_and_transfer(coin_vector, TEST_SENDER_ADDR);

        scenario.next_tx(TEST_SENDER_ADDR);
        let coin1 = scenario.take_from_sender<Coin<SUI>>();

        // result is `3` coins of balance `2`
        assert!(coin1.value() == 6, 0);
        assert!(coin.value() == 4, 0);

        test_utils::destroy(coin);
        test_utils::destroy(coin1);
        scenario_val.end();
    }
}

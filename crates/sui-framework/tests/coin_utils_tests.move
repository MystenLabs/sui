// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::test_coin_utils {
    use std::vector;
    use sui::test_scenario::Self;
    use sui::coin;
    use sui::coin_utils;
    use sui::sui::SUI;
    use sui::coin::Coin;

    const TEST_SENDER_ADDR: address = @0xA11CE;

    #[test]
    public entry fun test_transform_surplus() {
        // This tests a case where we request less total coin amount than the total supplied
        // This will lead to the request being fulfilled, but also excess coins

        let coin_vec = vector::empty<Coin<SUI>>();
        let amount_vec = vector::empty<u64>();

        let scenario = &mut test_scenario::begin(&TEST_SENDER_ADDR);
        let ctx = test_scenario::ctx(scenario);

        let total_amount = 0;
        let i = 0u64;

        while (i < 3) {
            vector::push_back(&mut coin_vec, coin::mint_for_testing(i*50 + 100, ctx));
            total_amount = total_amount + i*50 + 100;
            i = i + 1;
        };
        vector::push_back(&mut amount_vec, 10);
        vector::push_back(&mut amount_vec, 20);
        vector::push_back(&mut amount_vec, 30);
        vector::push_back(&mut amount_vec, 30);
        vector::push_back(&mut amount_vec, 30);

        let ret = coin_utils::transform_internal_for_testing(coin_vec, amount_vec, ctx);

        // Expected flow
        // amount_vec: [10, 20, 30, 30, 30]
        // coin_vec: [100, 150, 200]
        // -----------------------------------
        // for the first 4  coins of total val 90, we repeatedly split off coin_vec[0] since 90 < 100
        // This leaves coin[0] with 10 units
        // for the last coin of value 30, we cannot use this value so we use merge coin[0] + coin[1] = 160
        // We split off 30 and are left with 130
        // 

        let i = 0;
        let len = vector::length(&amount_vec);

        let seen_amount = 0;
        // Check that all the amounts we want are present in result
        while (i < len) {
            let coin = vector::remove(&mut ret, 0);
            let expected_amount = vector::borrow(&amount_vec, i);
            assert!(coin::value(&coin) == *expected_amount, 0);
            seen_amount = seen_amount + *expected_amount;
            coin::destroy_for_testing(coin);
            i = i + 1;
        };

        // Left over coins from splitting off 5 coins
        assert!(vector::length(&ret) == 2, 0);

        let coin = vector::pop_back(&mut ret);
        seen_amount = seen_amount + coin::value(&coin);
        coin::destroy_for_testing(coin);
        let coin = vector::pop_back(&mut ret);
        seen_amount = seen_amount + coin::value(&coin);
        coin::destroy_for_testing(coin);
        vector::destroy_empty(ret);
        assert!(seen_amount == total_amount, 0);
    }

    #[test]
    public entry fun test_transform_exact_amount() {
        // This tests a case where we request total request is total available
        // This will lead to the request being fulfilled, with no excess

        let coin_vec = vector::empty<Coin<SUI>>();
        let amount_vec = vector::empty<u64>();

        let scenario = &mut test_scenario::begin(&TEST_SENDER_ADDR);
        let ctx = test_scenario::ctx(scenario);

        let total_amount = 0;
        let i = 0u64;

        while (i < 3) {
            vector::push_back(&mut coin_vec, coin::mint_for_testing(i*50 + 100, ctx));
            total_amount = total_amount + i*50 + 100;
            i = i + 1;
        };
        vector::push_back(&mut amount_vec, 50);
        vector::push_back(&mut amount_vec, 100);
        vector::push_back(&mut amount_vec, 200);
        vector::push_back(&mut amount_vec, 100);

        let ret = coin_utils::transform_internal_for_testing(coin_vec, amount_vec, ctx);

        // Expected flow
        // amount_vec: [50, 100, 200, 100]
        // coin_vec: [100, 150, 200]
        // -----------------------------------
        // All will be satisfied eventually with nothing left

        let i = 0;
        let len = vector::length(&amount_vec);

        let seen_amount = 0;
        // Check that all the amounts we want are present in result
        while (i < len) {
            let coin = vector::remove(&mut ret, 0);
            let expected_amount = vector::borrow(&amount_vec, i);
            assert!(coin::value(&coin) == *expected_amount, 0);
            seen_amount = seen_amount + *expected_amount;
            coin::destroy_for_testing(coin);
            i = i + 1;
        };

        assert!(vector::length(&ret) == 0, 0);

        vector::destroy_empty(ret);
        assert!(seen_amount == total_amount, 0);
    }

    #[test]
    public entry fun test_transform_exact_amount_and_values() {
        // This tests a case where we request total request is total available
        // This will lead to the request being fulfilled, with no excess

        let coin_vec = vector::empty<Coin<SUI>>();
        let amount_vec = vector::empty<u64>();

        let scenario = &mut test_scenario::begin(&TEST_SENDER_ADDR);
        let ctx = test_scenario::ctx(scenario);

        let total_amount = 0;
        let i = 0u64;

        while (i < 3) {
            vector::push_back(&mut coin_vec, coin::mint_for_testing(i*50 + 100, ctx));
            total_amount = total_amount + i*50 + 100;
            i = i + 1;
        };
        vector::push_back(&mut amount_vec, 100);
        vector::push_back(&mut amount_vec, 150);
        vector::push_back(&mut amount_vec, 200);

        let ret = coin_utils::transform_internal_for_testing(coin_vec, amount_vec, ctx);

        // Expected flow
        // amount_vec: [100, 150, 200]
        // coin_vec: [100, 150, 200]
        // -----------------------------------
        // All will be satisfied eventually with nothing left

        let i = 0;
        let len = vector::length(&amount_vec);

        let seen_amount = 0;
        // Check that all the amounts we want are present in result
        while (i < len) {
            let coin = vector::remove(&mut ret, 0);
            let expected_amount = vector::borrow(&amount_vec, i);
            assert!(coin::value(&coin) == *expected_amount, 0);
            seen_amount = seen_amount + *expected_amount;
            coin::destroy_for_testing(coin);
            i = i + 1;
        };

        assert!(vector::length(&ret) == 0, 0);

        vector::destroy_empty(ret);
        assert!(seen_amount == total_amount, 0);
    }

    #[test]
    public entry fun test_transform_deficit() {
        // This tests a case where we request more total coin amount than the total supplied
        // This will lead to the request partially being fulfilled

        let coin_vec = vector::empty<Coin<SUI>>();
        let amount_vec = vector::empty<u64>();

        let scenario = &mut test_scenario::begin(&TEST_SENDER_ADDR);
        let ctx = test_scenario::ctx(scenario);

        let total_amount = 0;
        let i = 0u64;

        while (i < 3) {
            vector::push_back(&mut coin_vec, coin::mint_for_testing(i*50 + 100, ctx));
            total_amount = total_amount + i*50 + 100;
            i = i + 1;
        };
        vector::push_back(&mut amount_vec, 10);
        vector::push_back(&mut amount_vec, 120);
        vector::push_back(&mut amount_vec, 210);
        vector::push_back(&mut amount_vec, 170);
        vector::push_back(&mut amount_vec, 30);

        let ret = coin_utils::transform_internal_for_testing(coin_vec, amount_vec, ctx);

        // Expected flow
        // total_amount = 450
        // amount_vec: [10, 120, 210, 170, 30]
        // coin_vec: [100, 150, 200]
        // -----------------------------------
        // for the first 3  coins of total val 340, we can satisfy
        // but we cannot satisfy the 4th coin of value 170
        // We will also have a coin of value 10 left since 450-340 = 110

        let i = 0;

        let seen_amount = 0;
        // Check that all the amounts we want are present in result
        while (i < 3) {
            let coin = vector::remove(&mut ret, 0);
            let expected_amount = vector::borrow(&amount_vec, i);
            assert!(coin::value(&coin) == *expected_amount, 0);
            seen_amount = seen_amount + *expected_amount;
            coin::destroy_for_testing(coin);
            i = i + 1;
        };

        // Left over 1 coin from splitting off
        assert!(vector::length(&ret) == 1, 0);

        let coin = vector::pop_back(&mut ret);
        seen_amount = seen_amount + coin::value(&coin);
        assert!(coin::value(&coin) == 110, 0);
        coin::destroy_for_testing(coin);
        assert!(seen_amount == total_amount, 0);
        vector::destroy_empty(ret);
    }

    #[test]
    public entry fun test_transform_deficit_zero_value_coins() {
        // This tests a case where we request some amount while we have zero value coins
        // This will lead to the request not being fulfilled, but all
        // We will also merge the zero coins into 1

        let coin_vec = vector::empty<Coin<SUI>>();
        let amount_vec = vector::empty<u64>();

        let scenario = &mut test_scenario::begin(&TEST_SENDER_ADDR);
        let ctx = test_scenario::ctx(scenario);

        let total_amount = 0;
        let i = 0u64;

        while (i < 3) {
            vector::push_back(&mut coin_vec, coin::mint_for_testing(0, ctx));
            total_amount = total_amount + 0;
            i = i + 1;
        };
        vector::push_back(&mut amount_vec, 10);
        vector::push_back(&mut amount_vec, 120);
        vector::push_back(&mut amount_vec, 210);
        vector::push_back(&mut amount_vec, 170);
        vector::push_back(&mut amount_vec, 30);

        let ret = coin_utils::transform_internal_for_testing(coin_vec, amount_vec, ctx);

        // Expected flow
        // total_amount = 0
        // amount_vec: [10, 120, 210, 170, 30]
        // coin_vec: [0, 0, 0]
        // -----------------------------------
        // nothing is satisfied

        assert!(vector::length(&ret) == 1, 0);

        let coin = vector::pop_back(&mut ret);
        assert!(coin::value(&coin) == 0, 0);
        coin::destroy_for_testing(coin);
        vector::destroy_empty(ret);
    }


    #[test]
    public entry fun test_transform_deficit_no_value() {
        // This tests a case where we request some amount while we have none
        // This will lead to the request not being fulfilled

        let coin_vec = vector::empty<Coin<SUI>>();
        let amount_vec = vector::empty<u64>();

        let scenario = &mut test_scenario::begin(&TEST_SENDER_ADDR);
        let ctx = test_scenario::ctx(scenario);

        vector::push_back(&mut amount_vec, 10);
        vector::push_back(&mut amount_vec, 120);
        vector::push_back(&mut amount_vec, 210);
        vector::push_back(&mut amount_vec, 170);
        vector::push_back(&mut amount_vec, 30);

        let ret = coin_utils::transform_internal_for_testing(coin_vec, amount_vec, ctx);

        // Expected flow
        // total_amount = 0
        // amount_vec: [10, 120, 210, 170, 30]
        // coin_vec: []
        // -----------------------------------
        // nothing is satisfied

        assert!(vector::length(&ret) == 0, 0);

        vector::destroy_empty(ret);
    }

    #[test]
    public entry fun test_transform_deficit_into_single() {
        // This tests a case where we request 1 very large coin with more amount than we have.
        // Essentially a merge all

        let coin_vec = vector::empty<Coin<SUI>>();
        let amount_vec = vector::empty<u64>();

        let scenario = &mut test_scenario::begin(&TEST_SENDER_ADDR);
        let ctx = test_scenario::ctx(scenario);

        let total_amount = 0;
        let i = 0u64;

        while (i < 3) {
            vector::push_back(&mut coin_vec, coin::mint_for_testing(i*50 + 100, ctx));
            total_amount = total_amount + i*50 + 100;
            i = i + 1;
        };
        vector::push_back(&mut amount_vec, 10000000);

        let ret = coin_utils::transform_internal_for_testing(coin_vec, amount_vec, ctx);

        // Expected flow
        // Everything gets merged in an attempt to fulfil the request
        assert!(vector::length(&ret) == 1, 0);

        let coin = vector::pop_back(&mut ret);
        assert!(coin::value(&coin) == total_amount, 0);
        coin::destroy_for_testing(coin);
        vector::destroy_empty(ret);
    }

    #[test]
    public entry fun test_transform_single_into_multiple() {
        // This tests a case where we request multiple coins from one large coin
        // Essentially a split to N

        let coin_vec = vector::empty<Coin<SUI>>();
        let amount_vec = vector::empty<u64>();

        let scenario = &mut test_scenario::begin(&TEST_SENDER_ADDR);
        let ctx = test_scenario::ctx(scenario);

        let total_amount = 50000000000;

        vector::push_back(&mut coin_vec, coin::mint_for_testing(total_amount, ctx));

        let total_amount_req = 100000 + 120000 + 0 + 140000 + 1 + 2 + 30 + 150000;
        vector::push_back(&mut amount_vec, 100000);
        vector::push_back(&mut amount_vec, 120000);
        // Zero value coins are allowed if specified
        vector::push_back(&mut amount_vec, 0);
        vector::push_back(&mut amount_vec, 140000);
        vector::push_back(&mut amount_vec, 1);
        vector::push_back(&mut amount_vec, 2);
        vector::push_back(&mut amount_vec, 30);
        vector::push_back(&mut amount_vec, 150000);

        let ret = coin_utils::transform_internal_for_testing(coin_vec, amount_vec, ctx);

        // Expected flow
        // We have to do multiple splits but will end up fulfilling the requests with 1 surplus coin

        let i = 0;
        let seen_amount = 0;
        // Check that all the amounts we want are present in result
        while (i < 8) {
            let coin = vector::remove(&mut ret, 0);
            let expected_amount = vector::borrow(&amount_vec, i);
            assert!(coin::value(&coin) == *expected_amount, 0);
            seen_amount = seen_amount + *expected_amount;
            coin::destroy_for_testing(coin);
            i = i + 1;
        };

        // We must have one surplus
        assert!(vector::length(&ret) == 1, 0);
        let coin = vector::pop_back(&mut ret);
        assert!(coin::value(&coin) == total_amount - total_amount_req, 0);
        coin::destroy_for_testing(coin);
        vector::destroy_empty(ret);
    }
}
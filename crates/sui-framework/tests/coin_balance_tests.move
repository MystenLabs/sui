// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::test_coin {
    use sui::test_scenario::{Self, ctx};
    use sui::coin;
    use sui::balance;
    use sui::sui::SUI;
    use sui::locked_coin::LockedCoin;
    use sui::tx_context;
    use sui::locked_coin;
    use sui::coin::Coin;

    #[test]
    fun type_morphing() {
        let test = &mut test_scenario::begin(&@0x1);

        let balance = balance::zero<SUI>();
        let coin = coin::from_balance(balance, ctx(test));
        let balance = coin::into_balance(coin);

        balance::destroy_zero(balance);

        let coin = coin::mint_for_testing<SUI>(100, ctx(test));
        let balance_mut = coin::balance_mut(&mut coin);
        let sub_balance = balance::split(balance_mut, 50);

        assert!(balance::value(&sub_balance) == 50, 0);
        assert!(coin::value(&coin) == 50, 0);

        let balance = coin::into_balance(coin);
        balance::join(&mut balance, sub_balance);

        assert!(balance::value(&balance) == 100, 0);

        let coin = coin::from_balance(balance, ctx(test));
        coin::keep(coin, ctx(test));
    }

    const TEST_SENDER_ADDR: address = @0xA11CE;
    const TEST_RECIPIENT_ADDR: address = @0xB0B;

    #[test]
    public entry fun test_locked_coin_valid() {
        let scenario = &mut test_scenario::begin(&TEST_SENDER_ADDR);
        let ctx = test_scenario::ctx(scenario);
        let coin = coin::mint_for_testing<SUI>(42, ctx);

        test_scenario::next_tx(scenario, &TEST_SENDER_ADDR);
        // Lock up the coin until epoch 2.
        locked_coin::lock_coin(coin, TEST_RECIPIENT_ADDR, 2, test_scenario::ctx(scenario));

        // Advance the epoch by 2.
        test_scenario::next_epoch(scenario);
        test_scenario::next_epoch(scenario);
        assert!(tx_context::epoch(test_scenario::ctx(scenario)) == 2, 1);

        test_scenario::next_tx(scenario, &TEST_RECIPIENT_ADDR);
        let locked_coin = test_scenario::take_owned<LockedCoin<SUI>>(scenario);
        // The unlock should go through since epoch requirement is met.
        locked_coin::unlock_coin(locked_coin, test_scenario::ctx(scenario));

        test_scenario::next_tx(scenario, &TEST_RECIPIENT_ADDR);
        let unlocked_coin = test_scenario::take_owned<Coin<SUI>>(scenario);
        assert!(coin::value(&unlocked_coin) == 42, 2);
        coin::destroy_for_testing(unlocked_coin);
    }

    #[test]
    #[expected_failure(abort_code = 1)]
    public entry fun test_locked_coin_invalid() {
        let scenario = &mut test_scenario::begin(&TEST_SENDER_ADDR);
        let ctx = test_scenario::ctx(scenario);
        let coin = coin::mint_for_testing<SUI>(42, ctx);

        test_scenario::next_tx(scenario, &TEST_SENDER_ADDR);
        // Lock up the coin until epoch 2.
        locked_coin::lock_coin(coin, TEST_RECIPIENT_ADDR, 2, test_scenario::ctx(scenario));

        // Advance the epoch by 1.
        test_scenario::next_epoch(scenario);
        assert!(tx_context::epoch(test_scenario::ctx(scenario)) == 1, 1);

        test_scenario::next_tx(scenario, &TEST_RECIPIENT_ADDR);
        let locked_coin = test_scenario::take_owned<LockedCoin<SUI>>(scenario);
        // The unlock should fail.
        locked_coin::unlock_coin(locked_coin, test_scenario::ctx(scenario));
    }
}

// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::test_coin {
    use sui::TestScenario::{Self, ctx};
    use sui::Coin;
    use sui::balance;
    use sui::SUI::SUI;
    use sui::LockedCoin::LockedCoin;
    use sui::TxContext;
    use sui::LockedCoin;
    use sui::Coin::Coin;

    #[test]
    fun type_morphing() {
        let test = &mut TestScenario::begin(&@0x1);

        let balance = balance::zero<SUI>();
        let coin = Coin::from_balance(balance, ctx(test));
        let balance = Coin::into_balance(coin);

        balance::destroy_zero(balance);

        let coin = Coin::mint_for_testing<SUI>(100, ctx(test));
        let balance_mut = Coin::balance_mut(&mut coin);
        let sub_balance = balance::split(balance_mut, 50);

        assert!(balance::value(&sub_balance) == 50, 0);
        assert!(Coin::value(&coin) == 50, 0);

        let balance = Coin::into_balance(coin);
        balance::join(&mut balance, sub_balance);

        assert!(balance::value(&balance) == 100, 0);

        let coin = Coin::from_balance(balance, ctx(test));
        Coin::keep(coin, ctx(test));
    }

    const TEST_SENDER_ADDR: address = @0xA11CE;
    const TEST_RECIPIENT_ADDR: address = @0xB0B;

    #[test]
    public entry fun test_locked_coin_valid() {
        let scenario = &mut TestScenario::begin(&TEST_SENDER_ADDR);
        let ctx = TestScenario::ctx(scenario);
        let coin = Coin::mint_for_testing<SUI>(42, ctx);

        TestScenario::next_tx(scenario, &TEST_SENDER_ADDR);
        // Lock up the coin until epoch 2.
        LockedCoin::lock_coin(coin, TEST_RECIPIENT_ADDR, 2, TestScenario::ctx(scenario));

        // Advance the epoch by 2.
        TestScenario::next_epoch(scenario);
        TestScenario::next_epoch(scenario);
        assert!(TxContext::epoch(TestScenario::ctx(scenario)) == 2, 1);

        TestScenario::next_tx(scenario, &TEST_RECIPIENT_ADDR);
        let locked_coin = TestScenario::take_owned<LockedCoin<SUI>>(scenario);
        // The unlock should go through since epoch requirement is met.
        LockedCoin::unlock_coin(locked_coin, TestScenario::ctx(scenario));

        TestScenario::next_tx(scenario, &TEST_RECIPIENT_ADDR);
        let unlocked_coin = TestScenario::take_owned<Coin<SUI>>(scenario);
        assert!(Coin::value(&unlocked_coin) == 42, 2);
        Coin::destroy_for_testing(unlocked_coin);
    }

    #[test]
    #[expected_failure(abort_code = 1)]
    public entry fun test_locked_coin_invalid() {
        let scenario = &mut TestScenario::begin(&TEST_SENDER_ADDR);
        let ctx = TestScenario::ctx(scenario);
        let coin = Coin::mint_for_testing<SUI>(42, ctx);

        TestScenario::next_tx(scenario, &TEST_SENDER_ADDR);
        // Lock up the coin until epoch 2.
        LockedCoin::lock_coin(coin, TEST_RECIPIENT_ADDR, 2, TestScenario::ctx(scenario));

        // Advance the epoch by 1.
        TestScenario::next_epoch(scenario);
        assert!(TxContext::epoch(TestScenario::ctx(scenario)) == 1, 1);

        TestScenario::next_tx(scenario, &TEST_RECIPIENT_ADDR);
        let locked_coin = TestScenario::take_owned<LockedCoin<SUI>>(scenario);
        // The unlock should fail.
        LockedCoin::unlock_coin(locked_coin, TestScenario::ctx(scenario));
    }
}

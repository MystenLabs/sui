// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::coin_balance_tests {
    use sui::test_scenario::{Self, ctx};
    use sui::pay;
    use sui::coin;
    use sui::balance;
    use sui::sui::SUI;

    #[test]
    fun type_morphing() {
        let scenario = test_scenario::begin(@0x1);
        let test = &mut scenario;

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
        pay::keep(coin, ctx(test));
        test_scenario::end(scenario);
    }
}

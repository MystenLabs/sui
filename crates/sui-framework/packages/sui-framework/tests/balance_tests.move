// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::coin_balance_tests {
    use sui::test_scenario;
    use sui::pay;
    use sui::coin;
    use sui::balance;
    use sui::sui::SUI;
    use sui::test_utils;


    #[test]
    fun type_morphing() {
        let mut scenario = test_scenario::begin(@0x1);

        let balance = balance::zero<SUI>();
        let coin = balance.into_coin(scenario.ctx());
        let balance = coin.into_balance();

        balance.destroy_zero();

        let mut coin = coin::mint_for_testing<SUI>(100, scenario.ctx());
        let balance_mut = coin::balance_mut(&mut coin);
        let sub_balance = balance_mut.split(50);

        assert!(sub_balance.value() == 50);
        assert!(coin.value() == 50);

        let mut balance = coin.into_balance();
        balance.join(sub_balance);

        assert!(balance.value() == 100);

        let coin = balance.into_coin(scenario.ctx());
        pay::keep(coin, scenario.ctx());
        scenario.end();
    }

    #[test]
    fun test_balance() {
        let mut balance = balance::zero<SUI>();
        let another = balance::create_for_testing(1000);

        balance.join(another);

        assert!(balance.value() == 1000);

        let balance1 = balance.split(333);
        let balance2 = balance.split(333);
        let balance3 = balance.split(334);

        balance.destroy_zero();

        assert!(balance1.value() == 333);
        assert!(balance2.value() == 333);
        assert!(balance3.value() == 334);

        test_utils::destroy(balance1);
        test_utils::destroy(balance2);
        test_utils::destroy(balance3);
    }
}

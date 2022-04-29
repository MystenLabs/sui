// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module Sui::TestCoin {
    use Sui::TestScenario::{Self, ctx};
    use Sui::Coin;
    use Sui::Balance;
    use Sui::SUI::SUI;

    #[test]
    fun type_morphing() {
        let test = &mut TestScenario::begin(&@0x1);

        let balance = Balance::zero<SUI>();
        let coin = Coin::from_balance(balance, ctx(test));
        let balance = Coin::into_balance(coin);

        Balance::destroy_zero(balance);

        let coin = Coin::mint_for_testing<SUI>(100, ctx(test));
        let balance_mut = Coin::balance_mut(&mut coin);
        let sub_balance = Balance::split(balance_mut, 50);

        assert!(Balance::value(&sub_balance) == 50, 0);
        assert!(Coin::value(&coin) == 50, 0);

        let balance = Coin::into_balance(coin);
        Balance::join(&mut balance, sub_balance);

        assert!(Balance::value(&balance) == 100, 0);

        let coin = Coin::from_balance(balance, ctx(test));
        Coin::keep(coin, ctx(test));
    }
}

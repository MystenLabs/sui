// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module FungibleTokens::BASKETTests {
    use FungibleTokens::BASKET::{Self, Reserve};
    use FungibleTokens::MANAGED::MANAGED;
    use Sui::Coin;
    use Sui::SUI::SUI;
    use Sui::TestScenario;

    #[test]
    public fun test_mint_burn() {
        let user = @0xA;

        let scenario = &mut TestScenario::begin(&user);
        {
            let ctx = TestScenario::ctx(scenario);
            BASKET::init_for_testing(ctx);
        };
        TestScenario::next_tx(scenario, &user);
        {
            let reserve_wrapper = TestScenario::take_shared<Reserve>(scenario);
            let reserve = TestScenario::borrow_mut(&mut reserve_wrapper);
            let ctx = TestScenario::ctx(scenario);
            assert!(BASKET::total_supply(reserve) == 0, 0);

            let num_coins = 10;
            let sui = Coin::mint_for_testing<SUI>(num_coins, ctx);
            let managed = Coin::mint_for_testing<MANAGED>(num_coins, ctx);
            let basket = BASKET::mint(reserve, sui, managed, ctx);
            assert!(Coin::value(&basket) == num_coins, 1);
            assert!(BASKET::total_supply(reserve) == num_coins, 2);

            let (sui, managed) = BASKET::burn(reserve, basket, ctx);
            assert!(Coin::value(&sui) == num_coins, 3);
            assert!(Coin::value(&managed) == num_coins, 4);

            Coin::keep(sui, ctx);
            Coin::keep(managed, ctx);
            TestScenario::return_shared(scenario, reserve_wrapper);
        }
    }

}

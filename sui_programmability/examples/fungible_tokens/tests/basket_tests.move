// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module fungible_tokens::basket_tests {
    use fungible_tokens::basket::{Self, Reserve};
    use fungible_tokens::managed::MANAGED;
    use sui::coin;
    use sui::sui::SUI;
    use sui::test_scenario;

    #[test]
    public fun test_mint_burn() {
        let user = @0xA;

        let scenario = &mut test_scenario::begin(&user);
        {
            let ctx = test_scenario::ctx(scenario);
            basket::init_for_testing(ctx);
        };
        test_scenario::next_tx(scenario, &user);
        {
            let reserve_wrapper = test_scenario::take_shared<Reserve>(scenario);
            let reserve = test_scenario::borrow_mut(&mut reserve_wrapper);
            let ctx = test_scenario::ctx(scenario);
            assert!(basket::total_supply(reserve) == 0, 0);

            let num_coins = 10;
            let sui = coin::mint_for_testing<SUI>(num_coins, ctx);
            let managed = coin::mint_for_testing<MANAGED>(num_coins, ctx);
            let basket = basket::mint(reserve, sui, managed, ctx);
            assert!(coin::value(&basket) == num_coins, 1);
            assert!(basket::total_supply(reserve) == num_coins, 2);

            let (sui, managed) = basket::burn(reserve, basket, ctx);
            assert!(coin::value(&sui) == num_coins, 3);
            assert!(coin::value(&managed) == num_coins, 4);

            coin::keep(sui, ctx);
            coin::keep(managed, ctx);
            test_scenario::return_shared(scenario, reserve_wrapper);
        }
    }

}

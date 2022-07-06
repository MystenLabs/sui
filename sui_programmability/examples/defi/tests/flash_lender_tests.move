// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module defi::flash_lender_tests {
    use defi::flash_lender::{Self, AdminCap, FlashLender};
    use sui::coin;
    use sui::sui::SUI;
    use sui::test_scenario;

    #[test]
    fun flash_loan_example() {
        let admin = @0x1;
        let borrower = @0x2;

        // admin creates a flash lender with 100 coins and a fee of 1 coin
        let scenario = &mut test_scenario::begin(&admin);
        {
            let ctx = test_scenario::ctx(scenario);
            let coin = coin::mint_for_testing<SUI>(100, ctx);
            flash_lender::create(coin, 1, ctx);
        };
        // borrower requests and repays a loan of 10 coins + the fee
        test_scenario::next_tx(scenario, &borrower);
        {
            let lender_wrapper = test_scenario::take_shared<FlashLender<SUI>>(scenario);
            let lender = test_scenario::borrow_mut(&mut lender_wrapper);
            let ctx = test_scenario::ctx(scenario);

            let (loan, receipt) = flash_lender::loan(lender, 10, ctx);
            // in practice, borrower does something (e.g., arbitrage) to make a profit from the loan.
            // simulate this by min ting the borrower 5 coins.
            let profit = coin::mint_for_testing<SUI>(5, ctx);
            coin::join(&mut profit, loan);
            let to_keep = coin::take(coin::balance_mut(&mut profit), 4, ctx);
            coin::keep(to_keep, ctx);
            flash_lender::repay(lender, profit, receipt);

            test_scenario::return_shared(scenario, lender_wrapper);
        };
        // admin withdraws the 1 coin profit from lending
        test_scenario::next_tx(scenario, &admin);
        {
            let lender_wrapper = test_scenario::take_shared<FlashLender<SUI>>(scenario);
            let lender = test_scenario::borrow_mut(&mut lender_wrapper);
            let admin_cap = test_scenario::take_owned<AdminCap>(scenario);
            let ctx = test_scenario::ctx(scenario);

            // max loan size should have increased because of the fee payment
            assert!(flash_lender::max_loan(lender) == 101, 0);
            // withdraw 1 coin from the pool available for lending
            let coin = flash_lender::withdraw(lender, &admin_cap, 1, ctx);
            // max loan size should decrease accordingly
            assert!(flash_lender::max_loan(lender) == 100, 0);
            coin::keep(coin, ctx);

            test_scenario::return_shared(scenario, lender_wrapper);
            test_scenario::return_owned(scenario, admin_cap);
        }
    }
}

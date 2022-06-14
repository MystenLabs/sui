// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module defi::flash_lender_tests {
    use sui::Coin;
    use sui::SUI::SUI;
    use sui::TestScenario;

    use defi::flash_lender::{Self, AdminCap, FlashLender};

    #[test]
    public entry fun flash_loan_example() {
        let admin = @0x1;
        let borrower = @0x2;

        // admin creates a flash lender with 100 coins and a fee of 1 coin
        let scenario = &mut TestScenario::begin(&admin);
        {
            let ctx = TestScenario::ctx(scenario);
            let coin = Coin::mint_for_testing<SUI>(100, ctx);
            flash_lender::create(coin, 1, ctx);
        };
        // borrower requests and repays a loan of 10 coins + the fee
        TestScenario::next_tx(scenario, &borrower);
        {
            let lender_wrapper = TestScenario::take_shared<FlashLender<SUI>>(scenario);
            let lender = TestScenario::borrow_mut(&mut lender_wrapper);
            let ctx = TestScenario::ctx(scenario);

            let (loan, receipt) = flash_lender::loan(lender, 10, ctx);
            // in practice, borrower does something (e.g., arbitrage) to make a profit from the loan.
            // simulate this by min ting the borrower 5 coins.
            let profit = Coin::mint_for_testing<SUI>(5, ctx);
            Coin::join(&mut profit, loan);
            let to_keep = Coin::withdraw(Coin::balance_mut(&mut profit), 4, ctx);
            Coin::keep(to_keep, ctx);
            flash_lender::repay(lender, profit, receipt);

            TestScenario::return_shared(scenario, lender_wrapper);
        };
        // admin withdraws the 1 coin profit from lending
        TestScenario::next_tx(scenario, &admin);
        {
            let lender_wrapper = TestScenario::take_shared<FlashLender<SUI>>(scenario);
            let lender = TestScenario::borrow_mut(&mut lender_wrapper);
            let admin_cap = TestScenario::take_owned<AdminCap>(scenario);
            let ctx = TestScenario::ctx(scenario);

            // max loan size should have increased because of the fee payment
            assert!(flash_lender::max_loan(lender) == 101, 0);
            // withdraw 1 coin from the pool available for lending
            let coin = flash_lender::withdraw(lender, &admin_cap, 1, ctx);
            // max loan size should decrease accordingly
            assert!(flash_lender::max_loan(lender) == 100, 0);
            Coin::keep(coin, ctx);

            TestScenario::return_shared(scenario, lender_wrapper);
            TestScenario::return_owned(scenario, admin_cap);
        }
    }
}

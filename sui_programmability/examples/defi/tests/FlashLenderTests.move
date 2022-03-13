#[test_only]
module DeFi::FlashLenderTests {
    use DeFi::FlashLender::{Self, AdminCap, FlashLender};
    use Sui::Coin;
    use Sui::GAS::GAS;
    use Sui::TestScenario;

    #[test]
    fun flash_loan_example() {
        let admin = @0x1;
        let borrower = @0x2;

        // admin creates a flash lender with 100 coins and a fee of 1 coin
        let scenario = &mut TestScenario::begin(&admin);
        {
            let ctx = TestScenario::ctx(scenario);
            let coin = Coin::mint_for_testing<GAS>(100, ctx);
            FlashLender::create(coin, 1, ctx);
        };
        // borrower requests and repays a loan of 10 coins + the fee
        TestScenario::next_tx(scenario, &borrower);
        {
            let lender = TestScenario::remove_object<FlashLender<GAS>>(scenario);
            let ctx = TestScenario::ctx(scenario);

            let (loan, receipt) = FlashLender::loan(&mut lender, 10, ctx);
            // in practice, borrower does something (e.g., arbitrage) to make a profit from the loan.
            // simulate this by min ting the borrower 5 coins.
            let profit = Coin::mint_for_testing<GAS>(5, ctx);
            Coin::join(&mut profit, loan);
            let to_keep = Coin::withdraw(&mut profit, 4, ctx);
            Coin::keep(to_keep, ctx);
            FlashLender::repay(&mut lender, profit, receipt);

            TestScenario::return_object(scenario, lender);
        };
        // admin withdraws the 1 coin profit from lending
        TestScenario::next_tx(scenario, &admin);
        {
            let lender = TestScenario::remove_object<FlashLender<GAS>>(scenario);
            let admin_cap = TestScenario::remove_object<AdminCap>(scenario);
            let ctx = TestScenario::ctx(scenario);

            // max loan size should have increased because of the fee payment
            assert!(FlashLender::max_loan(&lender) == 101, 0);
            // withdraw 1 coin from the pool available for lending
            let coin = FlashLender::withdraw(&mut lender, &admin_cap, 1, ctx);
            // max loan size should decrease accordingly
            assert!(FlashLender::max_loan(&lender) == 100, 0);
            Coin::keep(coin, ctx);

            TestScenario::return_object(scenario, lender);
            TestScenario::return_object(scenario, admin_cap);
        }
    }
}
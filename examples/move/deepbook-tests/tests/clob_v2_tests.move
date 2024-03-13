// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only, allow(unused_variable, unused_function)]
/// Implements tests for the CLOB V2 module.
module deepbook::clob_v2_tests {
    use sui::sui::SUI;
    use deepbook::clob_v2::{Self as clob, Pool};

    use tests::test_runner::{Self as test, USD};

    // necessary for the test to work
    use fun test::place_limit as Pool.limit_order;

    #[test]
    fun create_pool() {
        let mut test = test::new();

        // TX1: Alice creates a pool with a tick size of 1, a lot size of 1,
        // a taker fee of 0, and a maker rebate of 0.
        let ctx = &mut test.next_tx(@alice);
        let (mut pool, cap) = test.new_pool()
            .tick_size(1)
            .lot_size(1)
            .taker_fee_rate(0)
            .maker_rebate_rate(0)
            .into<SUI, USD>(ctx);

        // TX2: Now Bob, who is a taker, wants to buy 1 SUI for 1 USD.
        let ctx = &mut test.next_tx(@bob);
        let bob = clob::create_account(ctx);
        let clock = test.clock(99, ctx);

        // Bob deposits 100 SUI.
        // Before placing the order, Bob needs to deposit the base currency.
        pool.deposit_base(test.mint(100_000_000_000, ctx), &bob);

        let order = test.new_order()
            .price(100)
            .quantity(2)
            .is_bid(false)
            .expiration(100);

        let (base_filled, quote_filled, is_success, _) = pool.limit_order(order, &clock, &bob, ctx);

        // TX3: Carl, who is a maker, wants to sell 1 SUI for 1 USD.
        let ctx = &mut test.next_tx(@carl);
        let carl = clob::create_account(ctx);
        pool.deposit_quote(test.mint_usd(100_000_000_000, ctx), &carl);

        let order = test.new_order()
            .price(100)
            .quantity(1)
            .is_bid(true)
            .expiration(100);

        let (base_filled, quote_filled, is_success, _) = pool.limit_order(order, &clock, &carl, ctx);

        test.destroy(clock)
            .destroy(pool)
            .destroy(carl)
            .destroy(bob)
            .destroy(cap);
    }
}

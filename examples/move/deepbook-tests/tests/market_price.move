// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only, allow(unused_variable, unused_function)]
module tests::market_price {
    use sui::sui::SUI;
    use tests::test_runner::{Self as test, USD};
    use deepbook::clob_v2::Pool;

    // necessary for the test to work
    use fun test::place_limit as Pool.limit_order;

    #[test] fun test_market_price() {

        // Alice creates a Pool and adds 1000 USDC and 1000 USDT
        let mut test = test::new();
        let ctx = &mut test.next_tx(@alice);
        let alice = test.create_account(ctx);
        let (mut pool, cap) = test.new_pool().into<SUI, USD>(ctx);

        pool.deposit_base(test.mint(100_000_000_000, ctx), &alice);
        pool.deposit_quote(test.mint(100_000_000_000, ctx), &alice);

        // No open orders, so the market price is None.
        let (bid_price, ask_price) = pool.get_market_price();

        assert!(bid_price.is_none(), 0);
        assert!(ask_price.is_none(), 1);

        // Bob places a bid for 100 USDC at 1.01 SUI
        let ctx = &mut test.next_tx(@bob);
        let bob = test.create_account(ctx);
        let clock = test.clock(0, ctx);
        let (price, quantity) = (1_010_000_000, 100);

        pool.deposit_quote(test.mint(price * quantity, ctx), &bob);

        let order = test.new_order()
            .price(price)
            .quantity(quantity)
            .is_bid(true)
            .expiration(10000);

        let (_, _, is_success, _) = pool.limit_order(order, &clock, &bob, ctx);

        assert!(is_success, 2);

        let (bid_price, ask_price) = pool.get_market_price();

        assert!(bid_price.is_some() && bid_price.borrow() == &price, 3);
        assert!(ask_price.is_none(), 4);

        std::debug::print(&bid_price);
        std::debug::print(&ask_price);

        test.destroy(alice)
            .destroy(pool)
            .destroy(cap)
            .destroy(bob)
            .destroy(clock);
    }
}

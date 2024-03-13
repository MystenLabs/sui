// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
#[allow(unused_variable, unused_function)]
/// Implements tests for the CLOB V2 module.
module deepbook::clob_v2_tests {
    use std::option::{Self, Option};
    use sui::tx_context::{Self, TxContext};
    use sui::coin::{Self, Coin};
    use sui::test_utils;
    use sui::sui::SUI;
    use sui::clock::{Self, Clock};

    use deepbook::clob_v2::{Self as clob, Pool};
    use deepbook::custodian_v2::AccountCap;

    public struct USD {}

    // #[test, expected_failure(abort_code = deepbook::clob_v2::EInvalidFee)]
    #[test]
    fun create_pool() {
        let mut test = new_test();

        // TX1: Alice creates a pool with a tick size of 1, a lot size of 1,
        // a taker fee of 0, and a maker rebate of 0.
        let ctx = &mut test.next_tx(@alice);
        let mut pool = new_pool<SUI, USD>(1, lot(1), 0, 0, ctx);

        // TX2: Now Bob, who is a taker, wants to buy 1 SUI for 1 USD.
        let ctx = &mut test.next_tx(@bob);
        let bob = clob::create_account(ctx);
        let clock = test.clock(99, ctx);

        // Bob deposits 100 SUI.
        pool.deposit_base(mint(100_000_000_000, ctx), &bob);

        let order = test.new_order()
            .price(100)
            .quantity(lot(2))
            .is_bid(false)
            .expiration(100);

        let (base_filled, quote_filled, is_success, _) = pool.limit_order(order, &clock, &bob, ctx);

        std::debug::print(&base_filled);
        std::debug::print(&quote_filled);
        std::debug::print(&is_success);

        // TX3: Carl, who is a maker, wants to sell 1 SUI for 1 USD.
        let ctx = &mut test.next_tx(@carl);
        let carl = clob::create_account(ctx);
        pool.deposit_quote(mint_usd(100_000_000_000, ctx), &carl);

        let order = test.new_order()
            .price(100)
            .quantity(lot(1))
            .is_bid(true)
            .expiration(100);

        let (base_filled, quote_filled, is_success, _) = pool.limit_order(order, &clock, &carl, ctx);

        // assert!(is_success, 0);
        std::debug::print(&base_filled);
        std::debug::print(&quote_filled);
        std::debug::print(&is_success);

        test.destroy(clock);
        test.destroy(pool);
        test.destroy(carl);
        test.destroy(bob);
    }

    #[test, expected_failure(abort_code = deepbook::clob_v2::EInvalidTickSizeLotSize)]
    fun create_pool_invalid_fee_fail() {
        let mut test = new_test();
        let ctx = &mut test.next_tx(@alice);
        let pool = new_pool<SUI, USD>(1, 1, 0, 1, ctx);

        abort 1337
    }

    fun lot(lot: u64): u64 { lot * 1_000_000_000 }
    fun def_tick(): u64 { 1 }

    fun new_pool<B, Q>(
        // what is the precision for an order?
        tick_size: u64,
        // what is the minimum order size?
        lot_size: u64,
        taker_fee_rate: u64,
        maker_rebate_rate: u64,
        ctx: &mut TxContext
    ): Pool<B, Q> {
        clob::create_customized_pool_with_return(
            tick_size,
            lot_size,
            taker_fee_rate,
            maker_rebate_rate,
            coin::mint_for_testing(100 * 1_000_000_000, ctx),
            ctx
        )
    }

    /// A mint function to mint SUI for testing.
    fun mint(amount: u64, ctx: &mut TxContext): Coin<SUI> {
        coin::mint_for_testing(amount, ctx)
    }

    /// A mint function to mint USD for testing.
    fun mint_usd(amount: u64, ctx: &mut TxContext): Coin<USD> {
        coin::mint_for_testing(amount, ctx)
    }

    /// A mint function to mint SUI for testing for pool creation
    fun mint_fee(ctx: &mut TxContext): Coin<SUI> {
        coin::mint_for_testing(100 * 1_000_000_000, ctx)
    }

    // === Tests Infra ===

    /// A test runner to generate transactions.
    public struct TestRunner has drop { seq: u64, time: u64 }

    /// Creates a new test runner to generate transactions.
    public fun new_test(): TestRunner { TestRunner { seq: 0, time: 0 } }

    /// Returns the clock with the given time.
    public fun clock(_self: &TestRunner, time: u64, ctx: &mut TxContext): Clock {
        let mut clock = clock::create_for_testing(ctx);
        clock::set_for_testing(&mut clock, time);
        clock
    }

    /// Destroys any object.
    public fun destroy<T>(_self: &TestRunner, v: T) {
        test_utils::destroy(v);
    }

    /// Creates a new transaction with the given sender. Make sure to keep the
    /// sequence number unique for each transaction.
    public fun next_tx(self: &mut TestRunner, sender: address): TxContext {
        self.seq = self.seq + 1;
        tx_context::new_from_hint(
            sender,
            self.seq,
            0, 0, 0
        )
    }

    // === DeepBook Specific ===

    public struct ClientOrder has drop {
        /// The order ID, user defined.
        id: Option<u64>,
        /// The price of the order, in base currency.
        price: Option<u64>,
        /// The quantity of the order, in quote currency.
        quantity: Option<u64>,
        /// The self matching prevention flag.
        self_matching_prevention: Option<u8>,
        /// Whether the order is a bid or ask.
        is_bid: Option<bool>,
        /// The expiration timestamp of the order.
        expiration: Option<u64>,
        /// The restriction of the order.
        restriction: Option<u8>,
    }

    public fun new_order(self: &TestRunner): ClientOrder {
        ClientOrder {
            id: option::none(),
            price: option::none(),
            quantity: option::none(),
            self_matching_prevention: option::none(),
            is_bid: option::none(),
            expiration: option::none(),
            restriction: option::none(),
        }
    }

    public fun id(mut self: ClientOrder, id: u64): ClientOrder { option::fill(&mut self.id, id); self }
    public fun price(mut self: ClientOrder, price: u64): ClientOrder { option::fill(&mut self.price, price); self }
    public fun quantity(mut self: ClientOrder, quantity: u64): ClientOrder { option::fill(&mut self.quantity, quantity); self }
    public fun self_matching_prevention(mut self: ClientOrder, smp: u8): ClientOrder { option::fill(&mut self.self_matching_prevention, smp); self }
    public fun is_bid(mut self: ClientOrder, is_bid: bool): ClientOrder { option::fill(&mut self.is_bid, is_bid); self }
    public fun expiration(mut self: ClientOrder, expiration: u64): ClientOrder { option::fill(&mut self.expiration, expiration); self }
    public fun restriction(mut self: ClientOrder, restriction: u8): ClientOrder { option::fill(&mut self.restriction, restriction); self }

    use fun place_limit as Pool.limit_order;

    public fun place_limit<B, Q>(
        pool: &mut Pool<B, Q>,
        order: ClientOrder,
        clock: &Clock,
        account: &AccountCap,
        ctx: &mut TxContext
    ): (u64, u64, bool, u64) {
        let ClientOrder {
            id,
            price,
            quantity,
            self_matching_prevention,
            is_bid,
            expiration,
            restriction,
        } = order;

        pool.place_limit_order(
            id.destroy_with_default(0),       // order_id
            price.destroy_some(),              // price
            quantity.destroy_some(),           // quantity
            self_matching_prevention.destroy_with_default(0), // self_matching_prevention
            is_bid.destroy_with_default(false), // is_bid
            expiration.destroy_with_default(0), // expiration timestamp
            restriction.destroy_with_default(0), // restriction
            clock,
            account,    // account
            ctx
        )
    }


}

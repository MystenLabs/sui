// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module tests::test_runner {
    use sui::sui::SUI;
    use sui::test_utils;
    use sui::coin::{Self, Coin};
    use sui::clock::{Self, Clock};

    use deepbook::clob_v2::{Self as clob, Pool, PoolOwnerCap};
    use deepbook::custodian_v2::AccountCap;

    // === Helper Types ===

    public struct USD {}

    // === Helper Functions ===

    /// Adds the 10^9 scale to the given value, required for deepbook operations.
    public fun scale(v: u64): u64 { v * 1_000_000_000 }

    // === Tests Infra ===

    /// A test runner to generate transactions.
    public struct TestRunner has drop { seq: u64, time: u64 }

    /// Creates a new test runner to generate transactions.
    public fun new(): TestRunner { TestRunner { seq: 0, time: 0 } }

    /// Returns the clock with the given time.
    public fun clock(_self: &TestRunner, time: u64, ctx: &mut TxContext): Clock {
        let mut clock = clock::create_for_testing(ctx);
        clock::set_for_testing(&mut clock, time);
        clock
    }

    /// Destroys any object.
    public fun destroy<T>(self: &TestRunner, v: T): &TestRunner {
        test_utils::destroy(v);
        self
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

    // === Helper Functions for TestRunner ===

    /// Generic mint function to mint any coin for testing.
    public fun mint<T>(_self: &TestRunner, amount: u64, ctx: &mut TxContext): Coin<T> {
        coin::mint_for_testing(amount, ctx)
    }

    /// A mint function to mint SUI for testing.
    public fun mint_sui(_self: &TestRunner, amount: u64, ctx: &mut TxContext): Coin<SUI> {
        coin::mint_for_testing(amount, ctx)
    }

    /// A mint function to mint USD for testing.
    public fun mint_usd(_self: &TestRunner, amount: u64, ctx: &mut TxContext): Coin<USD> {
        coin::mint_for_testing(amount, ctx)
    }

    /// A mint function to mint SUI for testing for pool creation
    public fun mint_fee(_self: &TestRunner, ctx: &mut TxContext): Coin<SUI> {
        coin::mint_for_testing(100 * 1_000_000_000, ctx)
    }

    /// Creates a new account for testing.
    public fun create_account(_self: &TestRunner, ctx: &mut TxContext): AccountCap {
        clob::create_account(ctx)
    }

    // === PoolBuilder ===

    /// A builder to create a pool.
    public struct PoolBuilder has drop {
        tick_size: Option<u64>,
        lot_size: Option<u64>,
        taker_fee_rate: Option<u64>,
        maker_rebate_rate: Option<u64>,
    }

    /// Creates a new pool builder.
    public fun new_pool(_self: &TestRunner): PoolBuilder {
        PoolBuilder {
            tick_size: option::none(),
            lot_size: option::none(),
            taker_fee_rate: option::none(),
            maker_rebate_rate: option::none(),
        }
    }

    /// Sets the tick size for the pool.
    /// The tick size is the minimum price increment for the pool.
    public fun tick_size(mut self: PoolBuilder, tick_size: u64): PoolBuilder {
        option::fill(&mut self.tick_size, tick_size);
        self
    }

    /// Sets the lot size for the pool, scaled by 10^9.
    /// The lot size is the minimum quantity increment for the pool.
    public fun lot_size(mut self: PoolBuilder, lot_size: u64): PoolBuilder {
        option::fill(&mut self.lot_size, scale(lot_size));
        self
    }

    /// Similar to `lot_size`, but does not scale the input.
    public fun lot_size_unsafe(mut self: PoolBuilder, lot_size: u64): PoolBuilder {
        option::fill(&mut self.lot_size, lot_size);
        self
    }

    /// Sets the taker fee rate for the pool.
    /// The taker fee rate is the fee rate for takers.
    public fun taker_fee_rate(mut self: PoolBuilder, taker_fee_rate: u64): PoolBuilder {
        option::fill(&mut self.taker_fee_rate, taker_fee_rate);
        self
    }

    /// Sets the maker rebate rate for the pool.
    public fun maker_rebate_rate(mut self: PoolBuilder, maker_rebate_rate: u64): PoolBuilder {
        option::fill(&mut self.maker_rebate_rate, maker_rebate_rate);
        self
    }

    /// Creates a pool with the given parameters.
    public fun into<B, Q>(self: PoolBuilder, ctx: &mut TxContext): (Pool<B, Q>, PoolOwnerCap) {
        let PoolBuilder {
            tick_size,
            lot_size,
            taker_fee_rate,
            maker_rebate_rate,
        } = self;

        clob::create_customized_pool_v2(
            tick_size.destroy_with_default(1),
            lot_size.destroy_with_default(1_000_000_000),
            taker_fee_rate.destroy_with_default(0),
            maker_rebate_rate.destroy_with_default(0),
            coin::mint_for_testing(100 * 1_000_000_000, ctx),
            ctx
        )
    }

    // === ClientOrder Type + Usage ===

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

    public fun new_order(_self: &TestRunner): ClientOrder {
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

    /// Sets the user order ID, optional.
    public fun id(mut self: ClientOrder, id: u64): ClientOrder {
        option::fill(&mut self.id, id);
        self
    }

    /// Price of the order, in base currency.
    public fun price(mut self: ClientOrder, price: u64): ClientOrder {
        option::fill(&mut self.price, price);
        self
    }

    /// Scales the input by 10^9 to match the deepbook scale.
    public fun quantity(mut self: ClientOrder, quantity: u64): ClientOrder {
        option::fill(&mut self.quantity, scale(quantity));
        self
    }

    /// Similar to `quantity`, but does not scale the input.
    public fun quantity_unsafe(mut self: ClientOrder, quantity: u64): ClientOrder {
        option::fill(&mut self.quantity, quantity);
        self
    }

    /// Sets the self matching prevention flag.
    public fun self_matching_prevention(mut self: ClientOrder, smp: u8): ClientOrder {
        option::fill(&mut self.self_matching_prevention, smp);
        self
    }

    /// Sets the order type to bid or ask.
    public fun is_bid(mut self: ClientOrder, is_bid: bool): ClientOrder {
        option::fill(&mut self.is_bid, is_bid);
        self
    }

    /// Sets the expiration timestamp, absolute time.
    public fun expiration(mut self: ClientOrder, expiration: u64): ClientOrder {
        option::fill(&mut self.expiration, expiration);
        self
    }

    /// Sets the restriction of the order.
    /// - 0: No restriction
    /// - 1: Immediate or Cancel
    /// - 2: Fill or Kill
    /// - 3 Post or Abort
    public fun restriction(mut self: ClientOrder, restriction: u8): ClientOrder {
        option::fill(&mut self.restriction, restriction);
        self
    }

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

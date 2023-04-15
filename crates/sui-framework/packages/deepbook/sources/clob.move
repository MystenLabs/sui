// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module deepbook::clob {
    use std::option;
    use std::type_name::{Self, TypeName};
    use std::vector;

    use sui::balance::{Self, Balance};
    use sui::clock::{Self, Clock};
    use sui::coin::{Self, Coin, join};
    use sui::event;
    use sui::linked_table::{Self, LinkedTable};
    use sui::object::{Self, UID, ID};
    use sui::sui::SUI;
    use sui::table::{Self, Table, contains, add, borrow_mut};
    use sui::transfer;
    use sui::tx_context::TxContext;

    use deepbook::critbit::{Self, CritbitTree, is_empty, borrow_mut_leaf_by_index, min_leaf, remove_leaf_by_index, max_leaf, next_leaf, previous_leaf, borrow_leaf_by_index, borrow_leaf_by_key, find_leaf, insert_leaf};
    use deepbook::custodian::{Self, Custodian, AccountCap, mint_account_cap};
    use deepbook::math::Self as clob_math;

    #[test_only]
    use sui::coin::mint_for_testing;
    #[test_only]
    use sui::test_scenario::{Self, Scenario};

    // <<<<<<<<<<<<<<<<<<<<<<<< Error codes <<<<<<<<<<<<<<<<<<<<<<<<
    const ENotImplemented: u64 = 1;
    const EInvalidFeeRateRebateRate: u64 = 2;
    const EInvalidOrderId: u64 = 3;
    const EUnauthorizedCancel: u64 = 4;
    const EInvalidPrice: u64 = 5;
    const EInvalidQuantity: u64 = 6;
    // Insufficient amount of base coin.
    const EInsufficientBaseCoin: u64 = 7;
    // Insufficient amount of quote coin.
    const EInsufficientQuoteCoin: u64 = 8;
    const EOrderCannotBeFullyFilled: u64 = 9;
    const EOrderCannotBeFullyPassive: u64 = 10;
    const EInvalidTickPrice: u64 = 11;
    const EInvalidUser: u64 = 12;
    const ENotEqual: u64 = 13;
    const EInvalidRestriction: u64 = 15;
    const ELevelNotEmpty: u64 = 16;
    const EInvalidPair: u64 = 17;
    const EInvalidBaseBalance: u64 = 18;
    const EInvalidBaseCoin: u64 = 19;
    const EInvalidFeeCoin: u64 = 20;
    const EInvalidFee: u64 = 21;
    const EInvalidExpireTimestamp: u64 = 22;

    // <<<<<<<<<<<<<<<<<<<<<<<< Error codes <<<<<<<<<<<<<<<<<<<<<<<<

    // <<<<<<<<<<<<<<<<<<<<<<<< Constants <<<<<<<<<<<<<<<<<<<<<<<<
    // Restrictions on limit orders.
    const N_RESTRICTIONS: u8 = 4;
    const NO_RESTRICTION: u8 = 0;
    // Mandates that whatever amount of an order that can be executed in the current transaction, be filled and then the rest of the order canceled.
    const IMMEDIATE_OR_CANCEL: u8 = 1;
    // Mandates that the entire order size be filled in the current transaction. Otherwise, the order is canceled.
    const FILL_OR_KILL: u8 = 2;
    // Mandates that the entire order be passive. Otherwise, cancel the order.
    const POST_OR_ABORT: u8 = 3;
    const MIN_BID_ORDER_ID: u64 = 0;
    const MIN_ASK_ORDER_ID: u64 = 1 << 63;
    const MIN_PRICE: u64 = 0;
    const MAX_PRICE: u64 = ((1u128 << 64 - 1) as u64);
    const TIMESTAMP_INF: u64 = ((1u128 << 64 - 1) as u64);
    const REFERENCE_TAKER_FEE_RATE: u64 = 5_000_000;
    const REFERENCE_MAKER_REBATE_RATE: u64 = 2_500_000;
    const FEE_AMOUNT_FOR_CREATE_POOL: u64 = 100 * 1_000_000_000; // 100 SUI

    // <<<<<<<<<<<<<<<<<<<<<<<< Constants <<<<<<<<<<<<<<<<<<<<<<<<

    // <<<<<<<<<<<<<<<<<<<<<<<< Events <<<<<<<<<<<<<<<<<<<<<<<<

    /// Emitted when a new pool is created
    struct PoolCreated has copy, store, drop {
        /// object ID of the newly created pool
        pool_id: ID,
        base_asset: TypeName,
        quote_asset: TypeName,
        taker_fee_rate: u64,
        // 10^9 scaling
        maker_rebate_rate: u64,
        tick_size: u64,
        lot_size: u64,
    }

    /// Emitted when a maker order is injected into the order book.
    struct OrderPlaced<phantom BaseAsset, phantom QuoteAsset> has copy, store, drop {
        /// object ID of the pool the order was placed on
        pool_id: ID,
        /// ID of the order within the pool
        order_id: u64,
        is_bid: bool,
        /// object ID of the `AccountCap` that placed the order
        owner: ID,
        base_asset_quantity_placed: u64,
        price: u64
    }

    /// Emitted when a maker order is canceled.
    struct OrderCanceled<phantom BaseAsset, phantom QuoteAsset> has copy, store, drop {
        /// object ID of the pool the order was placed on
        pool_id: ID,
        /// ID of the order within the pool
        order_id: u64,
        is_bid: bool,
        /// object ID of the `AccountCap` that placed the order
        owner: ID,
        base_asset_quantity_canceled: u64,
        price: u64
    }

    /// Emitted only when a maker order is filled.
    struct OrderFilled<phantom BaseAsset, phantom QuoteAsset> has copy, store, drop {
        /// object ID of the pool the order was placed on
        pool_id: ID,
        /// ID of the order within the pool
        order_id: u64,
        is_bid: bool,
        /// object ID of the `AccountCap` that placed the order
        owner: ID,
        total_quantity: u64,
        base_asset_quantity_filled: u64,
        base_asset_quantity_remaining: u64,
        price: u64
    }
    // <<<<<<<<<<<<<<<<<<<<<<<< Events <<<<<<<<<<<<<<<<<<<<<<<<

    struct Order has store, drop {
        // For each pool, order id is incremental and unique for each opening order.
        // Orders that are submitted earlier has lower order ids.
        // 64 bits are sufficient for order ids whereas 32 bits are not.
        // Assuming a maximum TPS of 100K/s of Sui chain, it would take (1<<63) / 100000 / 3600 / 24 / 365 = 2924712 years to reach the full capacity.
        // The highest bit of the order id is used to denote the order tyep, 0 for bid, 1 for ask.
        order_id: u64,
        // Only used for limit orders.
        price: u64,
        quantity: u64,
        is_bid: bool,
        // Order can only be cancelled by the owner.
        owner: ID,
        // Expiration timestamp in ms.
        expire_timestamp: u64,
    }

    struct TickLevel has store {
        price: u64,
        // The key is order order id.
        open_orders: LinkedTable<u64, Order>,
        // other price level info
    }

    struct Pool<phantom BaseAsset, phantom QuoteAsset> has key {
        // The key to the following Critbit Tree are order prices.
        id: UID,
        // All open bid orders.
        bids: CritbitTree<TickLevel>,
        // All open ask orders.
        asks: CritbitTree<TickLevel>,
        // Order id of the next bid order, starting from 0.
        next_bid_order_id: u64,
        // Order id of the next ask order, starting from 1<<63.
        next_ask_order_id: u64,
        // Map from user id -> (map from order id -> order price)
        usr_open_orders: Table<ID, LinkedTable<u64, u64>>,
        // taker_fee_rate should be strictly greater than maker_rebate_rate.
        // The difference between taker_fee_rate and maker_rabate_rate goes to the protocol.
        // 10^9 scaling
        taker_fee_rate: u64,
        // 10^9 scaling
        maker_rebate_rate: u64,
        tick_size: u64,
        lot_size: u64,
        // other pool info
        base_custodian: Custodian<BaseAsset>,
        quote_custodian: Custodian<QuoteAsset>,
        /// Stores the fee paid to create this pool. These funds are not accessible.
        creation_fee: Balance<SUI>,
        /// Stores the trading fees paid in `BaseAsset`. These funds are not accessible.
        base_asset_trading_fees: Balance<BaseAsset>,
         /// Stores the trading fees paid in `QuoteAsset`. These funds are not accessible.
        quote_asset_trading_fees: Balance<QuoteAsset>,
    }

    fun destroy_empty_level(level: TickLevel) {
        let TickLevel {
            price: _,
            open_orders: orders,
        } = level;

        linked_table::destroy_empty(orders);
    }

    public fun create_account(ctx: &mut TxContext): AccountCap {
        mint_account_cap(ctx)
    }

    fun create_pool_<BaseAsset, QuoteAsset>(
        taker_fee_rate: u64,
        maker_rebate_rate: u64,
        tick_size: u64,
        lot_size: u64,
        creation_fee: Balance<SUI>,
        ctx: &mut TxContext,
    ) {
        let base_type_name = type_name::get<BaseAsset>();
        let quote_type_name = type_name::get<QuoteAsset>();
        assert!(base_type_name != quote_type_name, EInvalidPair);
        assert!(taker_fee_rate >= maker_rebate_rate, EInvalidFeeRateRebateRate);
        let pool_uid = object::new(ctx);
        let pool_id = *object::uid_as_inner(&pool_uid);
        transfer::share_object(
            Pool<BaseAsset, QuoteAsset> {
                id: pool_uid,
                bids: critbit::new(ctx),
                asks: critbit::new(ctx),
                next_bid_order_id: MIN_BID_ORDER_ID,
                next_ask_order_id: MIN_ASK_ORDER_ID,
                usr_open_orders: table::new(ctx),
                taker_fee_rate,
                maker_rebate_rate,
                tick_size,
                lot_size,
                base_custodian: custodian::new<BaseAsset>(ctx),
                quote_custodian: custodian::new<QuoteAsset>(ctx),
                creation_fee,
                base_asset_trading_fees: balance::zero(),
                quote_asset_trading_fees: balance::zero(),
            }
        );
        event::emit(PoolCreated {
            pool_id,
            base_asset: base_type_name,
            quote_asset: quote_type_name,
            taker_fee_rate,
            maker_rebate_rate,
            tick_size,
            lot_size,
        })
    }

    public fun create_pool<BaseAsset, QuoteAsset>(
        tick_size: u64,
        lot_size: u64,
        creation_fee: Coin<SUI>,
        ctx: &mut TxContext,
    ) {
        assert!(coin::value(&creation_fee) == FEE_AMOUNT_FOR_CREATE_POOL, EInvalidFee);
        create_pool_<BaseAsset, QuoteAsset>(
            REFERENCE_TAKER_FEE_RATE,
            REFERENCE_MAKER_REBATE_RATE,
            tick_size,
            lot_size,
            coin::into_balance(creation_fee),
            ctx
        )
    }

    public fun deposit_base<BaseAsset, QuoteAsset>(
        pool: &mut Pool<BaseAsset, QuoteAsset>,
        coin: Coin<BaseAsset>,
        account_cap: &AccountCap
    ) {
        custodian::increase_user_available_balance(&mut pool.base_custodian, object::id(account_cap), coin::into_balance(coin))
    }

    public fun deposit_quote<BaseAsset, QuoteAsset>(
        pool: &mut Pool<BaseAsset, QuoteAsset>,
        coin: Coin<QuoteAsset>,
        account_cap: &AccountCap
    ) {
        custodian::increase_user_available_balance(&mut pool.quote_custodian, object::id(account_cap), coin::into_balance(coin))
    }

    public fun withdraw_base<BaseAsset, QuoteAsset>(
        pool: &mut Pool<BaseAsset, QuoteAsset>,
        quantity: u64,
        account_cap: &AccountCap,
        ctx: &mut TxContext
    ): Coin<BaseAsset> {
        custodian::withdraw_base_asset(&mut pool.base_custodian, quantity, account_cap, ctx)
    }

    public fun withdraw_quote<BaseAsset, QuoteAsset>(
        pool: &mut Pool<BaseAsset, QuoteAsset>,
        quantity: u64,
        account_cap: &AccountCap,
        ctx: &mut TxContext
    ): Coin<QuoteAsset> {
        custodian::withdraw_quote_asset(&mut pool.quote_custodian, quantity, account_cap, ctx)
    }

    // for smart routing
    public fun swap_exact_base_for_quote<BaseAsset, QuoteAsset>(
        pool: &mut Pool<BaseAsset, QuoteAsset>,
        quantity: u64,
        base_coin: Coin<BaseAsset>,
        quote_coin: Coin<QuoteAsset>,
        clock: &Clock,
        ctx: &mut TxContext,
    ): (Coin<BaseAsset>, Coin<QuoteAsset>, u64) {
        let original_val = coin::value(&quote_coin);
        let (ret_base_coin, ret_quote_coin) = place_market_order(
            pool,
            quantity,
            false,
            base_coin,
            quote_coin,
            clock,
            ctx
        );
        let ret_val = coin::value(&ret_quote_coin);
        (ret_base_coin, ret_quote_coin, ret_val - original_val)
    }

    // for smart routing
    public fun swap_exact_quote_for_base<BaseAsset, QuoteAsset>(
        pool: &mut Pool<BaseAsset, QuoteAsset>,
        quantity: u64,
        clock: &Clock,
        quote_coin: Coin<QuoteAsset>,
        ctx: &mut TxContext,
    ): (Coin<BaseAsset>, Coin<QuoteAsset>, u64) {
        let (base_asset_balance, quote_asset_balance) = match_bid_with_quote_quantity(
            pool,
            quantity,
            MAX_PRICE,
            clock::timestamp_ms(clock),
            coin::into_balance(quote_coin)
        );
        let val = balance::value(&base_asset_balance);
        (coin::from_balance(base_asset_balance, ctx), coin::from_balance(quote_asset_balance, ctx), val)
    }

    fun match_bid_with_quote_quantity<BaseAsset, QuoteAsset>(
        pool: &mut Pool<BaseAsset, QuoteAsset>,
        quantity: u64,
        price_limit: u64,
        current_timestamp: u64,
        quote_balance: Balance<QuoteAsset>,
    ): (Balance<BaseAsset>, Balance<QuoteAsset>) {
        // Base balance received by taker, taking into account of taker commission.
        // Need to individually keep track of the remaining base quantity to be filled to avoid infinite loop.
        let taker_quote_quantity_remaining = quantity;
        let base_balance_filled = balance::zero<BaseAsset>();
        let quote_balance_left = quote_balance;
        let all_open_orders = &mut pool.asks;
        if (critbit::is_empty(all_open_orders)) {
            return (base_balance_filled, quote_balance_left)
        };
        let (tick_price, tick_index) = min_leaf(all_open_orders);

        while (!is_empty<TickLevel>(all_open_orders) && tick_price <= price_limit) {
            let tick_level = borrow_mut_leaf_by_index(all_open_orders, tick_index);
            let order_id = *option::borrow(linked_table::front(&tick_level.open_orders));

            while (!linked_table::is_empty(&tick_level.open_orders)) {
                let maker_order = linked_table::borrow(&tick_level.open_orders, order_id);
                let maker_base_quantity = maker_order.quantity;
                let skip_order = false;

                if (maker_order.expire_timestamp <= current_timestamp) {
                    skip_order = true;
                    custodian::unlock_balance(&mut pool.base_custodian, maker_order.owner, maker_order.quantity);
                } else {
                    let (flag, maker_quote_quantity) = clob_math::mul_round(maker_base_quantity, maker_order.price);
                    if (flag) maker_quote_quantity = maker_quote_quantity + 1;
                    // filled_quote_quantity, subtract from taker total quote remaining in each loop, round up if needed
                    let filled_quote_quantity =
                        if (taker_quote_quantity_remaining >= maker_quote_quantity) { maker_quote_quantity }
                        else { taker_quote_quantity_remaining };
                    // filled_base_quantity, subtract from maker locked_base_balance, round up if needed
                    let (flag, filled_base_quantity) = clob_math::div_round(filled_quote_quantity, maker_order.price);
                    if (flag) filled_base_quantity = filled_base_quantity + 1;
                    // rebate_fee to maker, no need to round up
                    let maker_rebate = clob_math::mul(filled_base_quantity, pool.maker_rebate_rate);
                    let (is_round_down, taker_commission) = clob_math::mul_round(filled_base_quantity, pool.taker_fee_rate);
                    if (is_round_down) taker_commission = taker_commission + 1;

                    maker_base_quantity = maker_base_quantity - filled_base_quantity;

                    // maker in ask side, decrease maker's locked base asset, increase maker's available quote asset
                    taker_quote_quantity_remaining = taker_quote_quantity_remaining - filled_quote_quantity;
                    let locked_base_balance = custodian::decrease_user_locked_balance<BaseAsset>(
                        &mut pool.base_custodian,
                        maker_order.owner,
                        filled_base_quantity
                    );
                    let taker_commission_balance = balance::split(
                        &mut locked_base_balance,
                        taker_commission,
                    );
                    custodian::increase_user_available_balance<BaseAsset>(
                        &mut pool.base_custodian,
                        maker_order.owner,
                        balance::split(
                            &mut taker_commission_balance,
                            maker_rebate,
                        ),
                    );
                    balance::join(&mut pool.base_asset_trading_fees, taker_commission_balance);
                    balance::join(&mut base_balance_filled, locked_base_balance);

                    custodian::increase_user_available_balance<QuoteAsset>(
                        &mut pool.quote_custodian,
                        maker_order.owner,
                        balance::split(
                            &mut quote_balance_left,
                            filled_quote_quantity,
                        ),
                    );

                    event::emit(OrderFilled<BaseAsset, QuoteAsset> {
                        pool_id: *object::uid_as_inner(&pool.id),
                        order_id: maker_order.order_id,
                        is_bid: false,
                        owner: maker_order.owner,
                        total_quantity: maker_order.quantity,
                        base_asset_quantity_filled: filled_base_quantity,
                        base_asset_quantity_remaining: maker_base_quantity,
                        price: maker_order.price
                    })
                };

                if (skip_order || maker_base_quantity == 0) {
                    // Remove the maker order.
                    let old_order_id = order_id;
                    let maybe_order_id = linked_table::next(&tick_level.open_orders, order_id);
                    if (!option::is_none(maybe_order_id)) {
                        order_id = *option::borrow(maybe_order_id);
                    };
                    let usr_open_order_ids = table::borrow_mut(&mut pool.usr_open_orders, maker_order.owner);
                    linked_table::remove(usr_open_order_ids, old_order_id);
                    linked_table::remove(&mut tick_level.open_orders, old_order_id);
                } else {
                    // Update the maker order.
                    let maker_order_mut = linked_table::borrow_mut(
                        &mut tick_level.open_orders,
                        order_id);
                    maker_order_mut.quantity = maker_base_quantity;
                };
                if (taker_quote_quantity_remaining == 0) {
                    break
                };
            };
            if (linked_table::is_empty(&tick_level.open_orders)) {
                (tick_price, _) = next_leaf(all_open_orders, tick_price);
                destroy_empty_level(remove_leaf_by_index(all_open_orders, tick_index));
                (_, tick_index) = find_leaf(all_open_orders, tick_price);
            };
            if (taker_quote_quantity_remaining == 0) {
                break
            };
        };
        return (base_balance_filled, quote_balance_left)
    }

    fun match_bid<BaseAsset, QuoteAsset>(
        pool: &mut Pool<BaseAsset, QuoteAsset>,
        quantity: u64,
        price_limit: u64,
        current_timestamp: u64,
        quote_balance: Balance<QuoteAsset>,
    ): (Balance<BaseAsset>, Balance<QuoteAsset>) {
        let pool_id = *object::uid_as_inner(&pool.id);
        // Base balance received by taker, taking into account of taker commission.
        // Need to individually keep track of the remaining base quantity to be filled to avoid infinite loop.
        let taker_base_quantity_remaining = quantity;
        let base_balance_filled = balance::zero<BaseAsset>();
        let quote_balance_left = quote_balance;
        let all_open_orders = &mut pool.asks;
        if (critbit::is_empty(all_open_orders)) {
            return (base_balance_filled, quote_balance_left)
        };
        let (tick_price, tick_index) = min_leaf(all_open_orders);

        while (!is_empty<TickLevel>(all_open_orders) && tick_price <= price_limit) {
            let tick_level = borrow_mut_leaf_by_index(all_open_orders, tick_index);
            let order_id = *option::borrow(linked_table::front(&tick_level.open_orders));

            while (!linked_table::is_empty(&tick_level.open_orders)) {
                let maker_order = linked_table::borrow(&tick_level.open_orders, order_id);
                let maker_base_quantity = maker_order.quantity;
                let skip_order = false;

                if (maker_order.expire_timestamp <= current_timestamp) {
                    skip_order = true;
                    custodian::unlock_balance(&mut pool.base_custodian, maker_order.owner, maker_order.quantity);
                    emit_order_canceled<BaseAsset, QuoteAsset>(pool_id, maker_order);
                } else {
                    let filled_base_quantity =
                        if (taker_base_quantity_remaining >= maker_base_quantity) { maker_base_quantity }
                        else { taker_base_quantity_remaining };
                    // filled_quote_quantity to maker,  no need to round up
                    let filled_quote_quantity = clob_math::mul(filled_base_quantity, maker_order.price);

                    // rebate_fee to maker, no need to round up
                    let maker_rebate = clob_math::mul(filled_base_quantity, pool.maker_rebate_rate);
                    let (is_round_down, taker_commission) = clob_math::mul_round(filled_base_quantity, pool.taker_fee_rate);
                    if (is_round_down) taker_commission = taker_commission + 1;

                    maker_base_quantity = maker_base_quantity - filled_base_quantity;

                    // maker in ask side, decrease maker's locked base asset, increase maker's available quote asset
                    taker_base_quantity_remaining = taker_base_quantity_remaining - filled_base_quantity;
                    let locked_base_balance = custodian::decrease_user_locked_balance<BaseAsset>(
                        &mut pool.base_custodian,
                        maker_order.owner,
                        filled_base_quantity
                    );
                    let taker_commission_balance = balance::split(
                        &mut locked_base_balance,
                        taker_commission,
                    );
                    custodian::increase_user_available_balance<BaseAsset>(
                        &mut pool.base_custodian,
                        maker_order.owner,
                        balance::split(
                            &mut taker_commission_balance,
                            maker_rebate,
                        ),
                    );
                    balance::join(&mut pool.base_asset_trading_fees, taker_commission_balance);
                    balance::join(&mut base_balance_filled, locked_base_balance);

                    custodian::increase_user_available_balance<QuoteAsset>(
                        &mut pool.quote_custodian,
                        maker_order.owner,
                        balance::split(
                            &mut quote_balance_left,
                            filled_quote_quantity,
                        ),
                    );

                    event::emit(OrderFilled<BaseAsset, QuoteAsset> {
                        pool_id,
                        order_id: maker_order.order_id,
                        is_bid: false,
                        owner: maker_order.owner,
                        total_quantity: maker_order.quantity,
                        base_asset_quantity_filled: filled_base_quantity,
                        base_asset_quantity_remaining: maker_base_quantity,
                        price: maker_order.price
                    })
                };

                if (skip_order || maker_base_quantity == 0) {
                    // Remove the maker order.
                    let old_order_id = order_id;
                    let maybe_order_id = linked_table::next(&tick_level.open_orders, order_id);
                    if (!option::is_none(maybe_order_id)) {
                        order_id = *option::borrow(maybe_order_id);
                    };
                    let usr_open_order_ids = table::borrow_mut(&mut pool.usr_open_orders, maker_order.owner);
                    linked_table::remove(usr_open_order_ids, old_order_id);
                    linked_table::remove(&mut tick_level.open_orders, old_order_id);
                } else {
                    // Update the maker order.
                    let maker_order_mut = linked_table::borrow_mut(
                        &mut tick_level.open_orders,
                        order_id);
                    maker_order_mut.quantity = maker_base_quantity;
                };
                if (taker_base_quantity_remaining == 0) {
                    break
                };
            };
            if (linked_table::is_empty(&tick_level.open_orders)) {
                (tick_price, _) = next_leaf(all_open_orders, tick_price);
                destroy_empty_level(remove_leaf_by_index(all_open_orders, tick_index));
                (_, tick_index) = find_leaf(all_open_orders, tick_price);
            };
            if (taker_base_quantity_remaining == 0) {
                break
            };
        };
        return (base_balance_filled, quote_balance_left)
    }

    fun match_ask<BaseAsset, QuoteAsset>(
        pool: &mut Pool<BaseAsset, QuoteAsset>,
        price_limit: u64,
        current_timestamp: u64,
        base_balance: Balance<BaseAsset>,
    ): (Balance<BaseAsset>, Balance<QuoteAsset>) {
        let pool_id = *object::uid_as_inner(&pool.id);
        let base_balance_left = base_balance;
        // Base balance received by taker, taking into account of taker commission.
        let quote_balance_filled = balance::zero<QuoteAsset>();
        let all_open_orders = &mut pool.bids;
        if (critbit::is_empty(all_open_orders)) {
            return (base_balance_left, quote_balance_filled)
        };
        let (tick_price, tick_index) = max_leaf(all_open_orders);
        while (!is_empty<TickLevel>(all_open_orders) && tick_price >= price_limit) {
            let tick_level = borrow_mut_leaf_by_index(all_open_orders, tick_index);
            let order_id = *option::borrow(linked_table::front(&tick_level.open_orders));
            while (!linked_table::is_empty(&tick_level.open_orders)) {
                let maker_order = linked_table::borrow(&tick_level.open_orders, order_id);
                let maker_base_quantity = maker_order.quantity;
                let skip_order = false;

                if (maker_order.expire_timestamp <= current_timestamp) {
                    skip_order = true;
                    let maker_quote_quantity = clob_math::mul(maker_order.quantity, maker_order.price);
                    custodian::unlock_balance(&mut pool.quote_custodian, maker_order.owner, maker_quote_quantity);
                    emit_order_canceled<BaseAsset, QuoteAsset>(pool_id, maker_order);
                } else {
                    let taker_base_quantity_remaining = balance::value(&base_balance_left);
                    let filled_base_quantity =
                        if (taker_base_quantity_remaining >= maker_base_quantity) { maker_base_quantity }
                        else { taker_base_quantity_remaining };
                    // filled_quote_quantity from maker, need to round up, but do in decrease stage
                    let filled_quote_quantity = clob_math::mul(filled_base_quantity, maker_order.price);

                    // rebate_fee to maker, no need to round up
                    let maker_rebate = clob_math::mul(filled_quote_quantity, pool.maker_rebate_rate);
                    let (is_round_down, taker_commission) = clob_math::mul_round(filled_quote_quantity, pool.taker_fee_rate);
                    if (is_round_down) taker_commission = taker_commission + 1;

                    maker_base_quantity = maker_base_quantity - filled_base_quantity;
                    // maker in bid side, decrease maker's locked quote asset, increase maker's available base asset
                    let locked_quote_balance = custodian::decrease_user_locked_balance<QuoteAsset>(
                        &mut pool.quote_custodian,
                        maker_order.owner,
                        filled_quote_quantity
                    );
                    let taker_commission_balance = balance::split(
                        &mut locked_quote_balance,
                        taker_commission,
                    );
                    custodian::increase_user_available_balance<QuoteAsset>(
                        &mut pool.quote_custodian,
                        maker_order.owner,
                        balance::split(
                            &mut taker_commission_balance,
                            maker_rebate,
                        ),
                    );
                    balance::join(&mut pool.quote_asset_trading_fees, taker_commission_balance);
                    balance::join(&mut quote_balance_filled, locked_quote_balance);

                    custodian::increase_user_available_balance<BaseAsset>(
                        &mut pool.base_custodian,
                        maker_order.owner,
                        balance::split(
                            &mut base_balance_left,
                            filled_base_quantity,
                        ),
                    );

                    event::emit(OrderFilled<BaseAsset, QuoteAsset> {
                        pool_id: *object::uid_as_inner(&pool.id),
                        order_id: maker_order.order_id,
                        is_bid: true,
                        owner: maker_order.owner,
                        total_quantity: maker_order.quantity,
                        base_asset_quantity_filled: filled_base_quantity,
                        base_asset_quantity_remaining: maker_base_quantity,
                        price: maker_order.price
                    })
                };

                if (skip_order || maker_base_quantity == 0) {
                    // Remove the maker order.
                    let old_order_id = order_id;
                    let maybe_order_id = linked_table::next(&tick_level.open_orders, order_id);
                    if (!option::is_none(maybe_order_id)) {
                        order_id = *option::borrow(maybe_order_id);
                    };
                    let usr_open_order_ids = table::borrow_mut(&mut pool.usr_open_orders, maker_order.owner);
                    linked_table::remove(usr_open_order_ids, old_order_id);
                    linked_table::remove(&mut tick_level.open_orders, old_order_id);
                } else {
                    // Update the maker order.
                    let maker_order_mut = linked_table::borrow_mut(
                        &mut tick_level.open_orders,
                        order_id);
                    maker_order_mut.quantity = maker_base_quantity;
                };
                if (balance::value(&base_balance_left) == 0) {
                    break
                };
            };
            if (linked_table::is_empty(&tick_level.open_orders)) {
                (tick_price, _) = previous_leaf(all_open_orders, tick_price);
                destroy_empty_level(remove_leaf_by_index(all_open_orders, tick_index));
                (_, tick_index) = find_leaf(all_open_orders, tick_price);
            };
            if (balance::value(&base_balance_left) == 0) {
                break
            };
        };
        return (base_balance_left, quote_balance_filled)
    }

    /// Place a market order to the order book.
    public fun place_market_order<BaseAsset, QuoteAsset>(
        pool: &mut Pool<BaseAsset, QuoteAsset>,
        quantity: u64,
        is_bid: bool,
        base_coin: Coin<BaseAsset>,
        quote_coin: Coin<QuoteAsset>,
        clock: &Clock,
        ctx: &mut TxContext,
    ): (Coin<BaseAsset>, Coin<QuoteAsset>) {
        // If market bid order, match against the open ask orders. Otherwise, match against the open ask orders.
        // Take market bid order for example.
        // We first retrieve the PriceLevel with the lowest price by calling min_leaf on the asks Critbit Tree.
        // We then match the market order by iterating through open orders on that price level in ascending order of the order id.
        // Open orders that are being filled are removed from the order book.
        // We stop the iteration untill all quantities are filled.
        // If the total quantity of open orders at the lowest price level is not large enough to fully fill the market order,
        // we move on to the next price level by calling next_leaf on the asks Critbit Tree and repeat the same procedure.
        // Continue iterating over the price levels in ascending order until the market order is completely filled.
        // If ther market order cannot be completely filled even after consuming all the open ask orders,
        // the unfilled quantity will be cancelled.
        // Market ask order follows similar procedure.
        // The difference is that market ask order is matched against the open bid orders.
        // We start with the bid PriceLeve with the highest price by calling max_leaf on the bids Critbit Tree.
        // The inner loop for iterating over the open orders in ascending orders of order id is the same as above.
        // Then iterate over the price levels in descending order until the market order is completely filled.
        assert!(quantity % pool.lot_size == 0, EInvalidQuantity);
        if (is_bid) {
            let (base_balance_filled, quote_balance_left) = match_bid(
                pool,
                quantity,
                MAX_PRICE,
                clock::timestamp_ms(clock),
                coin::into_balance(quote_coin),
            );
            join(
                &mut base_coin,
                coin::from_balance(base_balance_filled, ctx),
            );
            quote_coin = coin::from_balance(quote_balance_left, ctx);
        } else {
            assert!(quantity <= coin::value(&base_coin), EInvalidBaseCoin);
            let (base_balance_left, quote_balance_filled) = match_ask(
                pool,
                MIN_PRICE,
                clock::timestamp_ms(clock),
                coin::into_balance(base_coin),
            );
            base_coin = coin::from_balance(base_balance_left, ctx);
            join(
                &mut quote_coin,
                coin::from_balance(quote_balance_filled, ctx),
            );
        };
        (base_coin, quote_coin)
    }

    /// Injects a maker order to the order book.
    /// Returns the order id.
    fun inject_limit_order<BaseAsset, QuoteAsset>(
        pool: &mut Pool<BaseAsset, QuoteAsset>,
        price: u64,
        quantity: u64,
        is_bid: bool,
        expire_timestamp: u64,
        account_cap: &AccountCap,
        ctx: &mut TxContext
    ): u64 {
        let user = object::id(account_cap);
        let order_id: u64;
        let open_orders: &mut CritbitTree<TickLevel>;
        if (is_bid) {
            let quote_quantity = clob_math::mul(quantity, price);
            custodian::lock_balance<QuoteAsset>(&mut pool.quote_custodian, account_cap, quote_quantity);
            order_id = pool.next_bid_order_id;
            pool.next_bid_order_id = pool.next_bid_order_id + 1;
            open_orders = &mut pool.bids;
        } else {
            custodian::lock_balance<BaseAsset>(&mut pool.base_custodian, account_cap, quantity);
            order_id = pool.next_ask_order_id;
            pool.next_ask_order_id = pool.next_ask_order_id + 1;
            open_orders = &mut pool.asks;
        };
        let order = Order {
            order_id,
            price,
            quantity,
            is_bid,
            owner: user,
            expire_timestamp,
        };
        let (tick_exists, tick_index) = find_leaf(open_orders, price);
        if (!tick_exists) {
            tick_index = insert_leaf(
                open_orders,
                price,
                TickLevel {
                    price,
                    open_orders: linked_table::new(ctx),
                });
        };

        let tick_level = borrow_mut_leaf_by_index(open_orders, tick_index);
        linked_table::push_back(&mut tick_level.open_orders, order_id, order);
        event::emit(OrderPlaced<BaseAsset, QuoteAsset> {
            pool_id: *object::uid_as_inner(&pool.id),
            order_id,
            is_bid,
            owner: user,
            base_asset_quantity_placed: quantity,
            price
        });
        if (!contains(&pool.usr_open_orders, user)) {
            add(&mut pool.usr_open_orders, user, linked_table::new(ctx));
        };
        linked_table::push_back(borrow_mut(&mut pool.usr_open_orders, user), order_id, price);

        return order_id
    }

    /// Place a limit order to the order book.
    /// Returns (base quantity filled, quote quantity filled, whether a maker order is being placed, order id of the maker order).
    public fun place_limit_order<BaseAsset, QuoteAsset>(
        pool: &mut Pool<BaseAsset, QuoteAsset>,
        price: u64,
        quantity: u64,
        is_bid: bool,
        expire_timestamp: u64, // Expiration timestamp in ms in absolute value inclusive.
        restriction: u8,
        clock: &Clock,
        account_cap: &AccountCap,
        ctx: &mut TxContext
    ): (u64, u64, bool, u64) {
        // If limit bid order, check whether the price is lower than the lowest ask order by checking the min_leaf of asks Critbit Tree.
        // If so, assign the sequence id of the order to be next_bid_order_id and increment next_bid_order_id by 1.
        // Inject the new order to the bids Critbit Tree according to the price and order id.
        // Otherwise, find the price level from the asks Critbit Tree that is no greater than the input price.
        // Match the bid order against the asks Critbit Tree in the same way as a market order but up until the price level found in the previous step.
        // If the bid order is not completely filled, inject the remaining quantity to the bids Critbit Tree according to the input price and order id.
        // If limit ask order, vice versa.
        assert!(quantity > 0, EInvalidQuantity);
        assert!(price % pool.tick_size == 0, EInvalidPrice);
        assert!(quantity % pool.lot_size == 0, EInvalidQuantity);
        assert!(expire_timestamp > clock::timestamp_ms(clock), EInvalidExpireTimestamp);
        let user = object::id(account_cap);
        let base_quantity_filled;
        let quote_quantity_filled;

        if (is_bid) {
            let quote_quantity_original = custodian::account_available_balance<QuoteAsset>(
                &pool.quote_custodian,
                user,
            );
            let quote_balance = custodian::decrease_user_available_balance<QuoteAsset>(
                &mut pool.quote_custodian,
                account_cap,
                quote_quantity_original,
            );
            let (base_balance_filled, quote_balance_left) = match_bid(
                pool,
                quantity,
                price,
                clock::timestamp_ms(clock),
                quote_balance,
            );
            base_quantity_filled = balance::value(&base_balance_filled);
            quote_quantity_filled = quote_quantity_original - balance::value(&quote_balance_left);

            custodian::increase_user_available_balance<BaseAsset>(
                &mut pool.base_custodian,
                user,
                base_balance_filled,
            );
            custodian::increase_user_available_balance<QuoteAsset>(
                &mut pool.quote_custodian,
                user,
                quote_balance_left,
            );
        } else {
            let base_balance = custodian::decrease_user_available_balance<BaseAsset>(
                &mut pool.base_custodian,
                account_cap,
                quantity,
            );
            let (base_balance_left, quote_balance_filled) = match_ask(
                pool,
                price,
                clock::timestamp_ms(clock),
                base_balance,
            );

            base_quantity_filled = quantity - balance::value(&base_balance_left);
            quote_quantity_filled = balance::value(&quote_balance_filled);

            custodian::increase_user_available_balance<BaseAsset>(
                &mut pool.base_custodian,
                user,
                base_balance_left,
            );
            custodian::increase_user_available_balance<QuoteAsset>(
                &mut pool.quote_custodian,
                user,
                quote_balance_filled,
            );
        };

        let order_id;
        if (restriction == IMMEDIATE_OR_CANCEL) {
            return (base_quantity_filled, quote_quantity_filled, false, 0)
        };
        if (restriction == FILL_OR_KILL) {
            assert!(base_quantity_filled == quantity, EOrderCannotBeFullyFilled);
            return (base_quantity_filled, quote_quantity_filled, false, 0)
        };
        if (restriction == POST_OR_ABORT) {
            assert!(base_quantity_filled == 0, EOrderCannotBeFullyPassive);
            order_id = inject_limit_order(pool, price, quantity, is_bid, expire_timestamp, account_cap, ctx);
            return (base_quantity_filled, quote_quantity_filled, true, order_id)
        } else {
            assert!(restriction == NO_RESTRICTION, EInvalidRestriction);
            order_id = inject_limit_order(
                pool,
                price,
                quantity - base_quantity_filled,
                is_bid,
                expire_timestamp,
                account_cap,
                ctx
            );
            return (base_quantity_filled, quote_quantity_filled, true, order_id)
        }
    }

    fun order_is_bid(order_id: u64): bool {
        return order_id < MIN_ASK_ORDER_ID
    }

    fun emit_order_canceled<BaseAsset, QuoteAsset>(
        pool_id: ID,
        order: &Order
    ) {
        event::emit(OrderCanceled<BaseAsset, QuoteAsset> {
            pool_id,
            order_id: order.order_id,
            is_bid: order.is_bid,
            owner: order.owner,
            base_asset_quantity_canceled: order.quantity,
            price: order.price
        })
    }

    /// Cancel and opening order.
    /// Abort if order_id is invalid or if the order is not submitted by the transaction sender.
    public fun cancel_order<BaseAsset, QuoteAsset>(
        pool: &mut Pool<BaseAsset, QuoteAsset>,
        order_id: u64,
        account_cap: &AccountCap
    ) {
        // First check the highest bit of the order id to see whether it's bid or ask.
        // Then retrieve the price using the order id.
        // Using the price to retrieve the corresponding PriceLevel from the bids / asks Critbit Tree.
        // Retrieve and remove the order from open orders of the PriceLevel.
        let user = object::id(account_cap);
        assert!(contains(&pool.usr_open_orders, user), EInvalidUser);
        let usr_open_orders = borrow_mut(&mut pool.usr_open_orders, user);
        assert!(linked_table::contains(usr_open_orders, order_id), EInvalidOrderId);
        let tick_price = *linked_table::borrow(usr_open_orders, order_id);
        let is_bid = order_is_bid(order_id);
        let (tick_exists, tick_index) = find_leaf(
            if (is_bid) { &pool.bids } else { &pool.asks },
            tick_price);
        assert!(tick_exists, EInvalidOrderId);
        let order = remove_order<BaseAsset, QuoteAsset>(
            if (is_bid) { &mut pool.bids } else { &mut pool.asks },
            usr_open_orders,
            tick_index,
            order_id,
            user
        );
        if (is_bid) {
            let balance_locked = clob_math::mul(order.quantity, order.price);
            custodian::unlock_balance(&mut pool.quote_custodian, user, balance_locked);
        } else {
            custodian::unlock_balance(&mut pool.base_custodian, user, order.quantity);
        };
        emit_order_canceled<BaseAsset, QuoteAsset>(*object::uid_as_inner(&pool.id), &order);
    }

    fun remove_order<BaseAsset, QuoteAsset>(
        open_orders: &mut CritbitTree<TickLevel>,
        usr_open_orders: &mut LinkedTable<u64, u64>,
        tick_index: u64,
        order_id: u64,
        user: ID,
    ): Order {
        linked_table::remove(usr_open_orders, order_id);
        let tick_level = borrow_leaf_by_index(open_orders, tick_index);
        assert!(linked_table::contains(&tick_level.open_orders, order_id), EInvalidOrderId);
        let mut_tick_level = borrow_mut_leaf_by_index(open_orders, tick_index);
        let order = linked_table::remove(&mut mut_tick_level.open_orders, order_id);
        assert!(order.owner == user, EUnauthorizedCancel);
        if (linked_table::is_empty(&mut_tick_level.open_orders)) {
            destroy_empty_level(remove_leaf_by_index(open_orders, tick_index));
        };
        order
    }

    public fun cancel_all_orders<BaseAsset, QuoteAsset>(
        pool: &mut Pool<BaseAsset, QuoteAsset>,
        account_cap: &AccountCap
    ) {
        let pool_id = *object::uid_as_inner(&pool.id);
        let user = object::id(account_cap);
        assert!(contains(&pool.usr_open_orders, user), 0);
        let usr_open_order_ids = table::borrow_mut(&mut pool.usr_open_orders, user);
        while (!linked_table::is_empty(usr_open_order_ids)) {
            let order_id = *option::borrow(linked_table::back(usr_open_order_ids));
            let order_price = *linked_table::borrow(usr_open_order_ids, order_id);
            let is_bid = order_is_bid(order_id);
            let open_orders =
                if (is_bid) { &mut pool.bids }
                else { &mut pool.asks };
            let (_, tick_index) = critbit::find_leaf(open_orders, order_price);
            let order = remove_order<BaseAsset, QuoteAsset>(
                open_orders,
                usr_open_order_ids,
                tick_index,
                order_id,
                user
            );
            if (is_bid) {
                let balance_locked = clob_math::mul(order.quantity, order.price);
                custodian::unlock_balance(&mut pool.quote_custodian, user, balance_locked);
            } else {
                custodian::unlock_balance(&mut pool.base_custodian, user, order.quantity);
            };
            emit_order_canceled<BaseAsset, QuoteAsset>(pool_id, &order);
        };
    }


    /// Batch cancel limit orders to save gas cost.
    /// Abort if any of the order_ids are not submitted by the sender.
    /// Skip any order_id that is invalid.
    /// Note that this function can reduce gas cost even further if caller has multiple orders at the same price level,
    /// and if orders with the same price are grouped together in the vector.
    /// For example, if we have the following order_id to price mapping, {0: 100., 1: 200., 2: 100., 3: 200.}.
    /// Grouping order_ids like [0, 2, 1, 3] would make it the most gas efficient.
    public fun batch_cancel_order<BaseAsset, QuoteAsset>(
        pool: &mut Pool<BaseAsset, QuoteAsset>,
        order_ids: vector<u64>,
        account_cap: &AccountCap
    ) {
        let pool_id = *object::uid_as_inner(&pool.id);
        // First group the order ids according to price level,
        // so that we don't have to retrieve the PriceLevel multiple times if there are orders at the same price level.
        // Iterate over each price level, retrieve the corresponding PriceLevel.
        // Iterate over the order ids that need to be canceled at that price level,
        // retrieve and remove the order from open orders of the PriceLevel.
        let user = object::id(account_cap);
        assert!(contains(&pool.usr_open_orders, user), 0);
        let tick_index: u64 = 0;
        let _open_orders = if (!critbit::is_empty(&pool.bids)) { &pool.bids } else { &pool.asks };
        let tick_price: u64 = borrow_leaf_by_index(_open_orders, tick_index).price;
        let n_order = vector::length(&order_ids);
        let i_order = 0;
        let usr_open_orders = borrow_mut(&mut pool.usr_open_orders, user);
        while (i_order < n_order) {
            let order_id = *vector::borrow(&order_ids, i_order);
            assert!(linked_table::contains(usr_open_orders, order_id), EInvalidOrderId);
            let new_tick_price = *linked_table::borrow(usr_open_orders, order_id);
            // let new_tick_price = order.price;
            let is_bid = order_is_bid(order_id);
            if (new_tick_price != tick_price) {
                tick_price = new_tick_price;
                let (tick_exists, new_tick_index) = find_leaf(
                    if (is_bid) { &pool.bids } else { &pool.asks },
                    tick_price
                );
                assert!(tick_exists, EInvalidTickPrice);
                tick_index = new_tick_index;
            };
            let order = remove_order<BaseAsset, QuoteAsset>(
                if (is_bid) { &mut pool.bids } else { &mut pool.asks },
                usr_open_orders,
                tick_index,
                order_id,
                user
            );
            if (is_bid) {
                let balance_locked = clob_math::mul(order.quantity, order.price);
                custodian::unlock_balance(&mut pool.quote_custodian, user, balance_locked);
            } else {
                custodian::unlock_balance(&mut pool.base_custodian, user, order.quantity);
            };
            emit_order_canceled<BaseAsset, QuoteAsset>(pool_id, &order);
            i_order = i_order + 1;
        }
    }

    public fun list_open_orders<BaseAsset, QuoteAsset>(
        pool: &Pool<BaseAsset, QuoteAsset>,
        account_cap: &AccountCap
    ): vector<Order> {
        let user = object::id(account_cap);
        let usr_open_order_ids = table::borrow(&pool.usr_open_orders, user);
        let open_orders = vector::empty<Order>();
        let order_id = linked_table::front(usr_open_order_ids);
        while (!option::is_none(order_id)) {
            let order_price = *linked_table::borrow(usr_open_order_ids, *option::borrow(order_id));
            let tick_level =
                if (order_is_bid(*option::borrow(order_id))) borrow_leaf_by_key(&pool.bids, order_price)
                else borrow_leaf_by_key(&pool.asks, order_price);
            let order = linked_table::borrow(&tick_level.open_orders, *option::borrow(order_id));
            vector::push_back(&mut open_orders, Order {
                order_id: order.order_id,
                price: order.price,
                quantity: order.quantity,
                is_bid: order.is_bid,
                owner: order.owner,
                expire_timestamp: order.expire_timestamp
            });
            order_id = linked_table::next(usr_open_order_ids, *option::borrow(order_id));
        };
        open_orders
    }

    /// query user balance inside custodian
    public fun account_balance<BaseAsset, QuoteAsset>(
        pool: &Pool<BaseAsset, QuoteAsset>,
        account_cap: &AccountCap
    ): (u64, u64, u64, u64) {
        let user = object::id(account_cap);
        let (base_avail, base_locked) = custodian::account_balance(&pool.base_custodian, user);
        let (quote_avail, quote_locked) = custodian::account_balance(&pool.quote_custodian, user);
        (base_avail, base_locked, quote_avail, quote_locked)
    }

    /// Enter a price range and return the level2 order depth of all valid prices within this price range in bid side
    /// returns two vectors of u64
    /// The previous is a list of all valid prices
    /// The latter is the corresponding depth list
    public fun get_level2_book_status_bid_side<BaseAsset, QuoteAsset>(
        pool: &Pool<BaseAsset, QuoteAsset>,
        price_low: u64,
        price_high: u64,
        clock: &Clock
    ): (vector<u64>, vector<u64>) {
        let (price_low_, _) = critbit::min_leaf(&pool.bids);
        if (price_low < price_low_) price_low = price_low_;
        let (price_high_, _) = critbit::max_leaf(&pool.bids);
        if (price_high > price_high_) price_high = price_high_;
        price_low = critbit::find_closest_key(&pool.bids, price_low);
        price_high = critbit::find_closest_key(&pool.bids, price_high);
        let price_vec = vector::empty<u64>();
        let depth_vec = vector::empty<u64>();
        if (price_low == 0) { return (price_vec, depth_vec) };
        while (price_low <= price_high) {
            let depth = get_level2_book_status<BaseAsset, QuoteAsset>(
                &pool.bids,
                price_low,
                clock::timestamp_ms(clock)
            );
            vector::push_back(&mut price_vec, price_low);
            vector::push_back(&mut depth_vec, depth);
            let (next_price, _) = critbit::next_leaf(&pool.bids, price_low);
            if (next_price == 0) { break }
            else { price_low = next_price };
        };
        (price_vec, depth_vec)
    }

    /// Enter a price range and return the level2 order depth of all valid prices within this price range in ask side
    /// returns two vectors of u64
    /// The previous is a list of all valid prices
    /// The latter is the corresponding depth list
    public fun get_level2_book_status_ask_side<BaseAsset, QuoteAsset>(
        pool: &Pool<BaseAsset, QuoteAsset>,
        price_low: u64,
        price_high: u64,
        clock: &Clock
    ): (vector<u64>, vector<u64>) {
        let (price_low_, _) = critbit::min_leaf(&pool.asks);
        if (price_low < price_low_) price_low = price_low_;
        let (price_high_, _) = critbit::max_leaf(&pool.asks);
        if (price_high > price_high_) price_high = price_high_;
        price_low = critbit::find_closest_key(&pool.asks, price_low);
        price_high = critbit::find_closest_key(&pool.asks, price_high);
        let price_vec = vector::empty<u64>();
        let depth_vec = vector::empty<u64>();
        if (price_low == 0) { return (price_vec, depth_vec) };
        while (price_low <= price_high) {
            let depth = get_level2_book_status<BaseAsset, QuoteAsset>(
                &pool.asks,
                price_low,
                clock::timestamp_ms(clock)
            );
            vector::push_back(&mut price_vec, price_low);
            vector::push_back(&mut depth_vec, depth);
            let (next_price, _) = critbit::next_leaf(&pool.asks, price_low);
            if (next_price == 0) { break }
            else { price_low = next_price };
        };
        (price_vec, depth_vec)
    }

    /// internal func to retrive single depth of a tick price
    fun get_level2_book_status<BaseAsset, QuoteAsset>(
        open_orders: &CritbitTree<TickLevel>,
        price: u64,
        time_stamp: u64
    ): u64 {
        let tick_level = critbit::borrow_leaf_by_key(open_orders, price);
        let tick_open_orders = &tick_level.open_orders;
        let depth = 0;
        let order_id = linked_table::front(tick_open_orders);
        let order: &Order;
        while (!option::is_none(order_id)) {
            order = linked_table::borrow(tick_open_orders, *option::borrow(order_id));
            if (order.expire_timestamp > time_stamp) depth = depth + order.quantity;
            order_id = linked_table::next(tick_open_orders, *option::borrow(order_id));
        };
        depth
    }

    public fun get_order_status<BaseAsset, QuoteAsset>(
        pool: &Pool<BaseAsset, QuoteAsset>,
        order_id: u64,
        account_cap: &AccountCap
    ): &Order {
        let user = object::id(account_cap);
        let usr_open_order_ids = table::borrow(&pool.usr_open_orders, user);
        let order_price = *linked_table::borrow(usr_open_order_ids, order_id);
        let open_orders =
            if (order_id < MIN_ASK_ORDER_ID) { &pool.bids }
            else { &pool.asks };
        let tick_level = critbit::borrow_leaf_by_key(open_orders, order_price);
        let tick_open_orders = &tick_level.open_orders;
        let order = linked_table::borrow(tick_open_orders, order_id);
        order
    }


    // Note that open orders and quotes can be directly accessed by loading in the entire Pool.

    #[test_only]
    const E_NULL: u64 = 0;

    #[test_only]
    struct USD {}

    #[test_only]
    public fun setup_test(
        taker_fee_rate: u64,
        maker_rebate_rate: u64,
        scenario: &mut Scenario,
        sender: address,
    ) {
        test_scenario::next_tx(scenario, sender);
        {
            clock::share_for_testing(clock::create_for_testing(test_scenario::ctx(scenario)));
        };

        test_scenario::next_tx(scenario, sender);
        {
            create_pool_<SUI, USD>(
                taker_fee_rate,
                maker_rebate_rate,
                1 * FLOAT_SCALING,
                1,
                balance::create_for_testing(FEE_AMOUNT_FOR_CREATE_POOL),
                test_scenario::ctx(scenario)
            );
        };
    }

    #[test_only]
    fun order_equal(
        order_left: &Order,
        order_right: &Order,
    ): bool {
        return (order_left.order_id == order_right.order_id) &&
            (order_left.price == order_right.price) &&
            (order_left.quantity == order_right.quantity) &&
            (order_left.is_bid == order_right.is_bid) &&
            (order_left.owner == order_right.owner)
    }

    #[test_only]
    fun contains_order(
        tree: &LinkedTable<u64, Order>,
        expected_order: &Order,
    ): bool {
        if (!linked_table::contains(tree, expected_order.order_id)) {
            return false
        };
        let order = linked_table::borrow(tree, expected_order.order_id);
        return order_equal(order, expected_order)
    }

    #[test_only]
    public fun check_tick_level(
        tree: &CritbitTree<TickLevel>,
        price: u64,
        open_orders: &vector<Order>,
    ) {
        let (tick_exists, tick_index) = find_leaf(tree, price);
        assert!(tick_exists, E_NULL);
        let tick_level = borrow_leaf_by_index(tree, tick_index);
        assert!(tick_level.price == price, E_NULL);
        let total_quote_amount: u64 = 0;
        assert!(linked_table::length(&tick_level.open_orders) == vector::length(open_orders), E_NULL);
        let i_order = 0;
        while (i_order < vector::length(open_orders)) {
            let order = vector::borrow(open_orders, i_order);
            total_quote_amount = total_quote_amount + order.quantity;
            assert!(order.price == price, E_NULL);
            assert!(contains_order(&tick_level.open_orders, order), E_NULL);
            i_order = i_order + 1;
        };
    }

    #[test_only]
    public fun check_empty_tick_level(
        tree: &CritbitTree<TickLevel>,
        price: u64,
    ) {
        let (tick_exists, _) = find_leaf(tree, price);
        assert!(!tick_exists, E_NULL);
    }


    #[test_only]
    public fun order_id(
        sequence_id: u64,
        is_bid: bool
    ): u64 {
        return if (is_bid) { MIN_BID_ORDER_ID + sequence_id } else { MIN_ASK_ORDER_ID + sequence_id }
    }

    #[test_only]
    public fun mint_account_cap_transfer(
        user: address,
        ctx: &mut TxContext
    ) {

        transfer::public_transfer(create_account(ctx), user);
    }

    #[test_only]
    public fun borrow_mut_custodian<BaseAsset, QuoteAsset>(
        pool: &mut Pool<BaseAsset, QuoteAsset>
    ): (&mut Custodian<BaseAsset>, &mut Custodian<QuoteAsset>) {
        (&mut pool.base_custodian, &mut pool.quote_custodian)
    }

    #[test_only]
    public fun borrow_custodian<BaseAsset, QuoteAsset>(
        pool: & Pool<BaseAsset, QuoteAsset>
    ): (&Custodian<BaseAsset>, &Custodian<QuoteAsset>) {
        (&pool.base_custodian, &pool.quote_custodian)
    }

    #[test_only]
    public fun test_match_bid<BaseAsset, QuoteAsset>(
        pool: &mut Pool<BaseAsset, QuoteAsset>,
        quantity: u64,
        price_limit: u64, // upper price limit if bid, lower price limit if ask, inclusive
        current_timestamp: u64,
    ): (u64, u64) {
        let quote_quantity_original = 1 << 63;
        let (base_balance_filled, quote_balance_left) = match_bid(
            pool,
            quantity,
            price_limit,
            current_timestamp,
            balance::create_for_testing<QuoteAsset>(quote_quantity_original),
        );
        let base_quantity_filled = balance::value(&base_balance_filled);
        let quote_quantity_filled = quote_quantity_original - balance::value(&quote_balance_left);
        balance::destroy_for_testing(base_balance_filled);
        balance::destroy_for_testing(quote_balance_left);
        return (base_quantity_filled, quote_quantity_filled)
    }

    #[test_only]
    public fun test_match_bid_with_quote_quantity<BaseAsset, QuoteAsset>(
        pool: &mut Pool<BaseAsset, QuoteAsset>,
        quantity: u64,
        price_limit: u64, // upper price limit if bid, lower price limit if ask, inclusive
        current_timestamp: u64,
    ): (u64, u64) {
        let quote_quantity_original = 1 << 63;
        let (base_balance_filled, quote_balance_left) = match_bid_with_quote_quantity(
            pool,
            quantity,
            price_limit,
            current_timestamp,
            balance::create_for_testing<QuoteAsset>(quote_quantity_original),
        );
        let base_quantity_filled = balance::value(&base_balance_filled);
        let quote_quantity_filled = quote_quantity_original - balance::value(&quote_balance_left);
        balance::destroy_for_testing(base_balance_filled);
        balance::destroy_for_testing(quote_balance_left);
        return (base_quantity_filled, quote_quantity_filled)
    }

    #[test_only]
    public fun test_match_ask<BaseAsset, QuoteAsset>(
        pool: &mut Pool<BaseAsset, QuoteAsset>,
        quantity: u64,
        price_limit: u64, // upper price limit if bid, lower price limit if ask, inclusive
        current_timestamp: u64,
    ): (u64, u64) {
        let (base_balance_left, quote_balance_filled) = match_ask(
            pool,
            price_limit,
            current_timestamp,
            balance::create_for_testing<BaseAsset>(quantity),
        );
        let base_quantity_filled = quantity - balance::value(&base_balance_left);
        let quote_quantity_filled = balance::value(&quote_balance_filled);
        balance::destroy_for_testing(base_balance_left);
        balance::destroy_for_testing(quote_balance_filled);
        return (base_quantity_filled, quote_quantity_filled)
    }

    #[test_only]
    public fun test_inject_limit_order<BaseAsset, QuoteAsset>(
        pool: &mut Pool<BaseAsset, QuoteAsset>,
        price: u64,
        quantity: u64,
        is_bid: bool,
        account_cap: &AccountCap,
        ctx: &mut TxContext
    ) {
        inject_limit_order(pool, price, quantity, is_bid, TIMESTAMP_INF, account_cap, ctx);
    }

    #[test_only]
    public fun test_inject_limit_order_with_expiration<BaseAsset, QuoteAsset>(
        pool: &mut Pool<BaseAsset, QuoteAsset>,
        price: u64,
        quantity: u64,
        is_bid: bool,
        expire_timestamp: u64,
        account_cap: &AccountCap,
        ctx: &mut TxContext
    ) {
        inject_limit_order(pool, price, quantity, is_bid, expire_timestamp, account_cap, ctx);
    }

    #[test_only]
    public fun get_pool_stat<BaseAsset, QuoteAsset>(
        pool: &Pool<BaseAsset, QuoteAsset>
    ): (u64, u64, &CritbitTree<TickLevel>, &CritbitTree<TickLevel>) {
        (
            pool.next_bid_order_id,
            pool.next_ask_order_id,
            &pool.bids,
            &pool.asks
        )
    }

    #[test_only]
    public fun get_usr_open_orders<BaseAsset, QuoteAsset>(
        pool: &Pool<BaseAsset, QuoteAsset>,
        owner: ID
    ): &LinkedTable<u64, u64> {
        assert!(contains(&pool.usr_open_orders, owner), 0);
        table::borrow(&pool.usr_open_orders, owner)
    }

    #[test_only]
    public fun test_construct_order(sequence_id: u64, price: u64, quantity: u64, is_bid: bool, owner: ID): Order {
        Order {
            order_id: order_id(sequence_id, is_bid),
            price,
            quantity,
            is_bid,
            owner,
            expire_timestamp: TIMESTAMP_INF,
        }
    }

    #[test_only]
    public fun test_construct_order_with_expiration(
        sequence_id: u64,
        price: u64,
        quantity: u64,
        is_bid: bool,
        owner: ID,
        expire_timestamp: u64
    ): Order {
        Order {
            order_id: order_id(sequence_id, is_bid),
            price,
            quantity,
            is_bid,
            owner,
            expire_timestamp,
        }
    }

    #[test_only]
    public fun check_usr_open_orders(
        usr_open_orders: &LinkedTable<u64, u64>,
        usr_open_orders_cmp: &vector<u64>,
    ) {
        assert!(2 * linked_table::length(usr_open_orders) == vector::length(usr_open_orders_cmp), 0);
        let i_order = 0;
        while (i_order < vector::length(usr_open_orders_cmp)) {
            let order_id = *vector::borrow(usr_open_orders_cmp, i_order);
            i_order = i_order + 1;
            assert!(linked_table::contains(usr_open_orders, order_id), 0);
            let price_cmp = *vector::borrow(usr_open_orders_cmp, i_order);
            let price = *linked_table::borrow(usr_open_orders, order_id);
            assert!(price_cmp == price, ENotEqual);
            i_order = i_order + 1;
        };
    }

    #[test_only]
    public fun test_remove_order<BaseAsset, QuoteAsset>(
        pool: &mut Pool<BaseAsset, QuoteAsset>,
        tick_index: u64,
        sequence_id: u64,
        is_bid: bool,
        owner: ID,
    ): Order {
        let order;
        if (is_bid) {
            order = remove_order<BaseAsset, QuoteAsset>(
                &mut pool.bids,
                borrow_mut(&mut pool.usr_open_orders, owner),
                tick_index,
                order_id(sequence_id, is_bid),
                owner
            )
        } else {
            order = remove_order<BaseAsset, QuoteAsset>(
                &mut pool.asks,
                borrow_mut(&mut pool.usr_open_orders, owner),
                tick_index,
                order_id(sequence_id, is_bid),
                owner
            )
        };
        order
    }

    const FLOAT_SCALING: u64 = 1000000000;

    #[test]
    #[expected_failure(abort_code = EOrderCannotBeFullyFilled)]
    fun test_place_limit_order_with_restrictions_FILL_OR_KILL_() {
        let owner: address = @0xAAAA;
        let alice: address = @0xBBBB;
        let bob: address = @0xCCCC;
        let test = test_scenario::begin(owner);
        test_scenario::next_tx(&mut test, owner);
        {
            setup_test(0, 0, &mut test, owner);
        };
        test_scenario::next_tx(&mut test, owner);
        {
            mint_account_cap_transfer(
                alice,
                test_scenario::ctx(&mut test)
            );
            mint_account_cap_transfer(
                bob,
                test_scenario::ctx(&mut test)
            );
        };
        test_scenario::next_tx(&mut test, alice);
        {
            let pool = test_scenario::take_shared<Pool<SUI, USD>>(&mut test);
            let clock = test_scenario::take_shared<Clock>(&test);
            let account_cap = test_scenario::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            custodian::deposit(
                &mut pool.base_custodian,
                mint_for_testing<SUI>(1000 * 100000000, test_scenario::ctx(&mut test)),
                account_cap_user
            );
            custodian::deposit(
                &mut pool.quote_custodian,
                mint_for_testing<USD>(10000 * 100000000, test_scenario::ctx(&mut test)),
                account_cap_user
            );
            place_limit_order<SUI, USD>(
                &mut pool,
                5 * FLOAT_SCALING,
                200 * 100000000,
                true,
                TIMESTAMP_INF,
                0,
                &clock,
                &account_cap,
                test_scenario::ctx(&mut test)
            );
            place_limit_order<SUI, USD>(
                &mut pool,
                4 * FLOAT_SCALING,
                200 * 100000000,
                true,
                TIMESTAMP_INF,
                0,
                &clock,
                &account_cap,
                test_scenario::ctx(&mut test)
            );
            place_limit_order<SUI, USD>(
                &mut pool,
                4 * FLOAT_SCALING,
                200 * 100000000,
                true,
                TIMESTAMP_INF,
                0,
                &clock,
                &account_cap,
                test_scenario::ctx(&mut test)
            );
            place_limit_order<SUI, USD>(
                &mut pool,
                10 * FLOAT_SCALING,
                1000 * 100000000,
                false,
                TIMESTAMP_INF,
                0,
                &clock,
                &account_cap,
                test_scenario::ctx(&mut test)
            );
            let (next_bid_order_id, next_ask_order_id, _, _) = get_pool_stat(&pool);
            assert!(next_bid_order_id == order_id(3, true), 0);
            assert!(next_ask_order_id == order_id(1, false), 0);
            custodian::assert_user_balance<USD>(
                &pool.quote_custodian,
                account_cap_user,
                7400 * 100000000,
                2600 * 100000000
            );
            custodian::assert_user_balance<SUI>(&pool.base_custodian, account_cap_user, 0, 1000 * 100000000);
            test_scenario::return_shared(pool);
            test_scenario::return_shared(clock);
            test_scenario::return_to_address<AccountCap>(alice, account_cap);
        };

        test_scenario::next_tx(&mut test, bob);
        {
            let pool = test_scenario::take_shared<Pool<SUI, USD>>(&mut test);
            let clock = test_scenario::take_shared<Clock>(&test);
            let account_cap = test_scenario::take_from_address<AccountCap>(&test, bob);
            let account_cap_user = object::id(&account_cap);
            custodian::deposit(
                &mut pool.base_custodian,
                mint_for_testing<SUI>(900 * 100000000, test_scenario::ctx(&mut test)),
                account_cap_user
            );
            custodian::assert_user_balance<SUI>(&pool.base_custodian, account_cap_user, 900 * 100000000, 0);
            place_limit_order<SUI, USD>(
                &mut pool,
                4 * FLOAT_SCALING,
                601 * 100000000,
                false,
                TIMESTAMP_INF,
                FILL_OR_KILL,
                &clock,
                &account_cap,
                test_scenario::ctx(&mut test)
            );
            custodian::assert_user_balance<USD>(&pool.quote_custodian, account_cap_user, 900 * 100000000, 0);
            test_scenario::return_shared(pool);
            test_scenario::return_shared(clock);
            test_scenario::return_to_address<AccountCap>(bob, account_cap);
        };
        test_scenario::end(test);
    }

    #[test]
    #[expected_failure(abort_code = EOrderCannotBeFullyPassive)]
    fun test_place_limit_order_with_restrictions_E_ORDER_CANNOT_BE_FULLY_PASSIVE_() {
        let owner: address = @0xAAAA;
        let alice: address = @0xBBBB;
        let bob: address = @0xCCCC;
        let test = test_scenario::begin(owner);
        test_scenario::next_tx(&mut test, owner);
        {
            setup_test(0, 0, &mut test, owner);
        };
        test_scenario::next_tx(&mut test, owner);
        {
            mint_account_cap_transfer(
                alice,
                test_scenario::ctx(&mut test)
            );
            mint_account_cap_transfer(
                bob,
                test_scenario::ctx(&mut test)
            );
        };
        test_scenario::next_tx(&mut test, alice);
        {
            let pool = test_scenario::take_shared<Pool<SUI, USD>>(&mut test);
            let clock = test_scenario::take_shared<Clock>(&test);
            let account_cap = test_scenario::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            custodian::deposit(
                &mut pool.base_custodian,
                mint_for_testing<SUI>(1000 * 100000000, test_scenario::ctx(&mut test)),
                account_cap_user
            );
            custodian::deposit(
                &mut pool.quote_custodian,
                mint_for_testing<USD>(10000 * 100000000, test_scenario::ctx(&mut test)),
                account_cap_user
            );
            place_limit_order<SUI, USD>(
                &mut pool,
                5 * FLOAT_SCALING,
                200 * 100000000,
                true,
                TIMESTAMP_INF,
                0,
                &clock,
                &account_cap,
                test_scenario::ctx(&mut test)
            );
            place_limit_order<SUI, USD>(
                &mut pool,
                4 * FLOAT_SCALING,
                200 * 100000000,
                true,
                TIMESTAMP_INF,
                0,
                &clock,
                &account_cap,
                test_scenario::ctx(&mut test)
            );
            place_limit_order<SUI, USD>(
                &mut pool,
                4 * FLOAT_SCALING,
                200 * 100000000,
                true,
                TIMESTAMP_INF,
                0,
                &clock,
                &account_cap,
                test_scenario::ctx(&mut test)
            );
            place_limit_order<SUI, USD>(
                &mut pool,
                10 * FLOAT_SCALING,
                1000 * 100000000,
                false,
                TIMESTAMP_INF,
                0,
                &clock,
                &account_cap,
                test_scenario::ctx(&mut test)
            );
            let (next_bid_order_id, next_ask_order_id, _, _) = get_pool_stat(&pool);
            assert!(next_bid_order_id == order_id(3, true), 0);
            assert!(next_ask_order_id == order_id(1, false), 0);
            custodian::assert_user_balance<USD>(
                &pool.quote_custodian,
                account_cap_user,
                7400 * 100000000,
                2600 * 100000000
            );
            custodian::assert_user_balance<SUI>(&pool.base_custodian, account_cap_user, 0, 1000 * 100000000);
            test_scenario::return_shared(pool);
            test_scenario::return_shared(clock);
            test_scenario::return_to_address<AccountCap>(alice, account_cap);
        };

        test_scenario::next_tx(&mut test, bob);
        {
            let pool = test_scenario::take_shared<Pool<SUI, USD>>(&mut test);
            let clock = test_scenario::take_shared<Clock>(&test);
            let account_cap = test_scenario::take_from_address<AccountCap>(&test, bob);
            let account_cap_user = object::id(&account_cap);
            custodian::deposit(
                &mut pool.base_custodian,
                mint_for_testing<SUI>(900 * 100000000, test_scenario::ctx(&mut test)),
                account_cap_user
            );
            custodian::assert_user_balance<SUI>(&pool.base_custodian, account_cap_user, 900 * 100000000, 0);
            place_limit_order<SUI, USD>(
                &mut pool,
                4 * FLOAT_SCALING,
                601 * 100000000,
                false,
                TIMESTAMP_INF,
                POST_OR_ABORT,
                &clock,
                &account_cap,
                test_scenario::ctx(&mut test)
            );
            custodian::assert_user_balance<SUI>(&pool.base_custodian, account_cap_user, 900 * 100000000, 0);
            test_scenario::return_shared(pool);
            test_scenario::return_shared(clock);
            test_scenario::return_to_address<AccountCap>(bob, account_cap);
        };
        test_scenario::end(test);
    }

    #[test]
    fun test_place_limit_order_with_restrictions_IMMEDIATE_OR_CANCEL() {
        let owner: address = @0xAAAA;
        let alice: address = @0xBBBB;
        let bob: address = @0xCCCC;
        let test = test_scenario::begin(owner);
        test_scenario::next_tx(&mut test, owner);
        {
            setup_test(0, 0, &mut test, owner);
        };
        test_scenario::next_tx(&mut test, owner);
        {
            mint_account_cap_transfer(
                alice,
                test_scenario::ctx(&mut test)
            );
            mint_account_cap_transfer(
                bob,
                test_scenario::ctx(&mut test)
            );
        };
        test_scenario::next_tx(&mut test, alice);
        {
            let pool = test_scenario::take_shared<Pool<SUI, USD>>(&mut test);
            let clock = test_scenario::take_shared<Clock>(&test);
            let account_cap = test_scenario::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            custodian::deposit(
                &mut pool.base_custodian,
                mint_for_testing<SUI>(1000 * 100000000, test_scenario::ctx(&mut test)),
                account_cap_user
            );
            custodian::deposit(
                &mut pool.quote_custodian,
                mint_for_testing<USD>(10000 * 100000000, test_scenario::ctx(&mut test)),
                account_cap_user
            );
            place_limit_order<SUI, USD>(
                &mut pool,
                5 * FLOAT_SCALING,
                200 * 100000000,
                true,
                TIMESTAMP_INF,
                0,
                &clock,
                &account_cap,
                test_scenario::ctx(&mut test)
            );
            place_limit_order<SUI, USD>(
                &mut pool,
                4 * FLOAT_SCALING,
                200 * 100000000,
                true,
                TIMESTAMP_INF,
                0,
                &clock,
                &account_cap,
                test_scenario::ctx(&mut test)
            );
            place_limit_order<SUI, USD>(
                &mut pool,
                4 * FLOAT_SCALING,
                200 * 100000000,
                true,
                TIMESTAMP_INF,
                0,
                &clock,
                &account_cap,
                test_scenario::ctx(&mut test)
            );

            let (base_filled, quote_filled, maker_injected, maker_order_id) = place_limit_order<SUI, USD>(
                &mut pool,
                10 * FLOAT_SCALING,
                1000 * 100000000,
                false,
                TIMESTAMP_INF,
                0,
                &clock,
                &account_cap,
                test_scenario::ctx(&mut test)
            );
            assert!(base_filled == 0, E_NULL);
            assert!(quote_filled == 0, E_NULL);
            assert!(maker_injected, E_NULL);
            assert!(maker_order_id == order_id(0, false), E_NULL);

            let (next_bid_order_id, next_ask_order_id, _, _) = get_pool_stat(&pool);
            assert!(next_bid_order_id == order_id(3, true), 0);
            assert!(next_ask_order_id == order_id(1, false), 0);
            custodian::assert_user_balance<USD>(
                &pool.quote_custodian,
                account_cap_user,
                7400 * 100000000,
                2600 * 100000000
            );
            custodian::assert_user_balance<SUI>(&pool.base_custodian, account_cap_user, 0, 1000 * 100000000);
            test_scenario::return_shared(pool);
            test_scenario::return_shared(clock);
            test_scenario::return_to_address<AccountCap>(alice, account_cap);
        };

        test_scenario::next_tx(&mut test, bob);
        {
            let pool = test_scenario::take_shared<Pool<SUI, USD>>(&mut test);
            let clock = test_scenario::take_shared<Clock>(&test);
            let account_cap = test_scenario::take_from_address<AccountCap>(&test, bob);
            let account_cap_user = object::id(&account_cap);
            custodian::deposit(
                &mut pool.base_custodian,
                mint_for_testing<SUI>(900 * 100000000, test_scenario::ctx(&mut test)),
                account_cap_user
            );
            custodian::assert_user_balance<SUI>(&pool.base_custodian, account_cap_user, 900 * 100000000, 0);

            let (base_filled, quote_filled, maker_injected, _) = place_limit_order<SUI, USD>(
                &mut pool,
                4 * FLOAT_SCALING,
                800 * 100000000,
                false,
                TIMESTAMP_INF,
                IMMEDIATE_OR_CANCEL,
                &clock,
                &account_cap,
                test_scenario::ctx(&mut test)
            );
            assert!(base_filled == 600 * 100000000, E_NULL);
            assert!(quote_filled == 2600 * 100000000, E_NULL);
            assert!(!maker_injected, E_NULL);

            custodian::assert_user_balance<SUI>(&pool.base_custodian, account_cap_user, 300 * 100000000, 0);
            {
                let (_, _, bids, _) = get_pool_stat(&pool);
                check_empty_tick_level(bids, 4 * FLOAT_SCALING);
            };
            test_scenario::return_shared(pool);
            test_scenario::return_shared(clock);
            test_scenario::return_to_address<AccountCap>(bob, account_cap);
        };
        test_scenario::end(test);
    }

    #[test]
    #[expected_failure(abort_code = EInvalidPair)]
    fun test_create_pool_invalid_pair() {
        let owner: address = @0xAAAA;
        let test = test_scenario::begin(owner);
        test_scenario::next_tx(&mut test, owner);
        {
            setup_test(0, 0, &mut test, owner);
        };
        // create pool which is already exist fail
        test_scenario::next_tx(&mut test, owner);
        {
            create_pool_<SUI, SUI>(
                REFERENCE_TAKER_FEE_RATE,
                REFERENCE_MAKER_REBATE_RATE,
                1,
                1,
                balance::create_for_testing(FEE_AMOUNT_FOR_CREATE_POOL),
                test_scenario::ctx(&mut test)
            );
        };
        test_scenario::end(test);
    }

    #[test]
    fun test_place_market_bid_with_zero_qty() {
        use sui::test_utils::destroy;

        let owner: address = @0xAAAA;
        let alice: address = @0xBBBB;
        let bob: address = @0xCCCC;
        let ted: address = @0xDDDD;

        let test = test_scenario::begin(owner);
        test_scenario::next_tx(&mut test, owner);
        // Create capabilites
        {
            setup_test(0, 0, &mut test, owner);
        };
        test_scenario::next_tx(&mut test, owner);
        {
            mint_account_cap_transfer(alice, test_scenario::ctx(&mut test));
            mint_account_cap_transfer(bob, test_scenario::ctx(&mut test));
            mint_account_cap_transfer(ted, test_scenario::ctx(&mut test));
        };

        // Post Alice limit orders
        test_scenario::next_tx(&mut test, alice);
        {
            let pool = test_scenario::take_shared<Pool<SUI, USD>>(&mut test);
            let clock = test_scenario::take_shared<Clock>(&test);
            let account_cap = test_scenario::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            custodian::deposit(
                &mut pool.base_custodian,
                mint_for_testing<SUI>(1000, test_scenario::ctx(&mut test)),
                account_cap_user
            );
            custodian::deposit(
                &mut pool.quote_custodian,
                mint_for_testing<USD>(1000, test_scenario::ctx(&mut test)),
                account_cap_user
            );
            place_limit_order<SUI, USD>(
                &mut pool,
                9 * FLOAT_SCALING,
                1,
                false,
                clock::timestamp_ms(&clock) + 1,
                0,
                &clock,
                &account_cap,
                test_scenario::ctx(&mut test)
            );
            place_limit_order<SUI, USD>(
                &mut pool,
                11 * FLOAT_SCALING,
                3,
                false,
                clock::timestamp_ms(&clock) + 1000000,
                0,
                &clock,
                &account_cap,
                test_scenario::ctx(&mut test)
            );
            test_scenario::return_shared(pool);
            test_scenario::return_shared(clock);
            test_scenario::return_to_address<AccountCap>(alice, account_cap);
        };

        // At this point, the ask book is as follows:
        // 1: price 9, qty=1, expiry <now>, from Alice
        // 2: price 11, qty=3, expiry <later>, from Alice

        // Post Bob limit orders
        test_scenario::next_tx(&mut test, bob);
        {
            let pool = test_scenario::take_shared<Pool<SUI, USD>>(&mut test);
            let clock = test_scenario::take_shared<Clock>(&test);
            let account_cap = test_scenario::take_from_address<AccountCap>(&test, bob);
            let account_cap_user = object::id(&account_cap);
            custodian::deposit(
                &mut pool.base_custodian,
                mint_for_testing<SUI>(1000, test_scenario::ctx(&mut test)),
                account_cap_user
            );
            custodian::deposit(
                &mut pool.quote_custodian,
                mint_for_testing<USD>(1000, test_scenario::ctx(&mut test)),
                account_cap_user
            );
            let (_, _, _, _) = place_limit_order<SUI, USD>(
                &mut pool,
                9 * FLOAT_SCALING,
                2,
                false,
                clock::timestamp_ms(&clock) + 1000000,
                0,
                &clock,
                &account_cap,
                test_scenario::ctx(&mut test)
            );
            test_scenario::return_shared(pool);
            test_scenario::return_shared(clock);
            test_scenario::return_to_address<AccountCap>(bob, account_cap);
        };

        // At this point, the ask book is as follows:
        // 1: price 9, qty=1, expiry <now>, from Alice
        // 2: price 9, qty=2, expiry <later>, from Bob
        // 3: price 11, qty=3, expiry <later>, from Alice

        // Send Ted market order
        test_scenario::next_tx(&mut test, ted);
        {
            let pool = test_scenario::take_shared<Pool<SUI, USD>>(&mut test);
            let clock = test_scenario::take_shared<Clock>(&test);

            // Advance clock
            clock::increment_for_testing(&mut clock, 10);

            let account_cap = test_scenario::take_from_address<AccountCap>(&test, ted);
            let account_cap_user = object::id(&account_cap);
            custodian::deposit(
                &mut pool.base_custodian,
                mint_for_testing<SUI>(1000, test_scenario::ctx(&mut test)),
                account_cap_user
            );
            custodian::deposit(
                &mut pool.quote_custodian,
                mint_for_testing<USD>(1000, test_scenario::ctx(&mut test)),
                account_cap_user
            );

            let (base, quote) = place_market_order<SUI, USD>(
                &mut pool,
                0,
                true,
                mint_for_testing<SUI>(0, test_scenario::ctx(&mut test)),
                mint_for_testing<USD>(0, test_scenario::ctx(&mut test)),
                &clock,
                test_scenario::ctx(&mut test)
            );

            destroy(base);
            destroy(quote);
            test_scenario::return_shared(pool);
            test_scenario::return_shared(clock);
            test_scenario::return_to_address<AccountCap>(ted, account_cap);
        };

        // At this point, the ask should be as follows:
        // ~~~: price 9, qty=1, expiry <now>, from Alice (removed because expired)
        // 1: price 9, qty=2, expiry <later>, from Bob
        // 2: price 11, qty=3, expiry <later>, from Alice
        test_scenario::next_tx(&mut test, ted);
        {
            use std::vector::push_back;
            use std::vector::empty;

            let bob_account_cap = test_scenario::take_from_address<AccountCap>(&test, bob);
            let bob_account_cap_user = object::id(&bob_account_cap);
            let clock = test_scenario::take_shared<Clock>(&test);

            let pool = test_scenario::take_shared<Pool<SUI, USD>>(&mut test);
            let (_, _, _, asks) = get_pool_stat(&pool);

            let orders_expected = empty<Order>();
            push_back(&mut orders_expected, test_construct_order_with_expiration(
                2,
                9 * FLOAT_SCALING,
                2,
                false,
                bob_account_cap_user,
                clock::timestamp_ms(&clock) + 1000000,
            ));
            // Note that the order quantity is 1 instead of the expected 2 and the order was found!
            check_tick_level(asks, 9 * FLOAT_SCALING, &orders_expected);

            test_scenario::return_shared(pool);
            test_scenario::return_shared(clock);
            test_scenario::return_to_address<AccountCap>(bob, bob_account_cap);
        };

        test_scenario::end(test);
    }

    #[test]
    fun test_place_market_ask_with_zero_qty() {
        use sui::test_utils::destroy;

        let owner: address = @0xAAAA;
        let alice: address = @0xBBBB;
        let bob: address = @0xCCCC;
        let ted: address = @0xDDDD;

        let test = test_scenario::begin(owner);
        test_scenario::next_tx(&mut test, owner);
        // Create capabilites
        {
            setup_test(0, 0, &mut test, owner);
        };
        test_scenario::next_tx(&mut test, owner);
        {
            mint_account_cap_transfer(alice, test_scenario::ctx(&mut test));
            mint_account_cap_transfer(bob, test_scenario::ctx(&mut test));
            mint_account_cap_transfer(ted, test_scenario::ctx(&mut test));
        };

        // Post Alice limit orders
        test_scenario::next_tx(&mut test, alice);
        {
            let pool = test_scenario::take_shared<Pool<SUI, USD>>(&mut test);
            let clock = test_scenario::take_shared<Clock>(&test);
            let account_cap = test_scenario::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            custodian::deposit(
                &mut pool.base_custodian,
                mint_for_testing<SUI>(1000, test_scenario::ctx(&mut test)),
                account_cap_user
            );
            custodian::deposit(
                &mut pool.quote_custodian,
                mint_for_testing<USD>(1000, test_scenario::ctx(&mut test)),
                account_cap_user
            );
            place_limit_order<SUI, USD>(
                &mut pool,
                11 * FLOAT_SCALING,
                1,
                true,
                clock::timestamp_ms(&clock) + 1,
                0,
                &clock,
                &account_cap,
                test_scenario::ctx(&mut test)
            );
            place_limit_order<SUI, USD>(
                &mut pool,
                9 * FLOAT_SCALING,
                3,
                true,
                clock::timestamp_ms(&clock) + 1000000,
                0,
                &clock,
                &account_cap,
                test_scenario::ctx(&mut test)
            );
            test_scenario::return_shared(pool);
            test_scenario::return_shared(clock);
            test_scenario::return_to_address<AccountCap>(alice, account_cap);
        };

        // At this point, the ask book is as follows:
        // 1: price 11, qty=1, expiry <now>, from Alice
        // 2: price 9, qty=3, expiry <later>, from Alice

        // Post Bob limit orders
        test_scenario::next_tx(&mut test, bob);
        {
            let pool = test_scenario::take_shared<Pool<SUI, USD>>(&mut test);
            let clock = test_scenario::take_shared<Clock>(&test);
            let account_cap = test_scenario::take_from_address<AccountCap>(&test, bob);
            let account_cap_user = object::id(&account_cap);
            custodian::deposit(
                &mut pool.base_custodian,
                mint_for_testing<SUI>(1000, test_scenario::ctx(&mut test)),
                account_cap_user
            );
            custodian::deposit(
                &mut pool.quote_custodian,
                mint_for_testing<USD>(1000, test_scenario::ctx(&mut test)),
                account_cap_user
            );
            let (_, _, _, _) = place_limit_order<SUI, USD>(
                &mut pool,
                11 * FLOAT_SCALING,
                2,
                true,
                clock::timestamp_ms(&clock) + 1000000,
                0,
                &clock,
                &account_cap,
                test_scenario::ctx(&mut test)
            );
            test_scenario::return_shared(pool);
            test_scenario::return_shared(clock);
            test_scenario::return_to_address<AccountCap>(bob, account_cap);
        };

        // At this point, the bid book is as follows:
        // 1: price 11, qty=1, expiry <now>, from Alice
        // 2: price 11, qty=2, expiry <later>, from Bob
        // 3: price 9, qty=3, expiry <later>, from Alice

        // Send Ted market order
        test_scenario::next_tx(&mut test, ted);
        {
            let pool = test_scenario::take_shared<Pool<SUI, USD>>(&mut test);
            let clock = test_scenario::take_shared<Clock>(&test);

            // Advance clock
            clock::increment_for_testing(&mut clock, 10);

            let account_cap = test_scenario::take_from_address<AccountCap>(&test, ted);
            let account_cap_user = object::id(&account_cap);
            custodian::deposit(
                &mut pool.base_custodian,
                mint_for_testing<SUI>(1000, test_scenario::ctx(&mut test)),
                account_cap_user
            );
            custodian::deposit(
                &mut pool.quote_custodian,
                mint_for_testing<USD>(1000, test_scenario::ctx(&mut test)),
                account_cap_user
            );

            let (base, quote) = place_market_order<SUI, USD>(
                &mut pool,
                0,
                false,
                mint_for_testing<SUI>(0, test_scenario::ctx(&mut test)),
                mint_for_testing<USD>(0, test_scenario::ctx(&mut test)),
                &clock,
                test_scenario::ctx(&mut test)
            );

            destroy(base);
            destroy(quote);
            test_scenario::return_shared(pool);
            test_scenario::return_shared(clock);
            test_scenario::return_to_address<AccountCap>(ted, account_cap);
        };

        // At this point, the ask should be as follows:
        // ~~~: price 11, qty=1, expiry <now>, from Alice (removed because expired)
        // 1: price 11, qty=2, expiry <later>, from Bob
        // 2: price 9, qty=3, expiry <later>, from Alice
        test_scenario::next_tx(&mut test, ted);
        {
            use std::vector::push_back;
            use std::vector::empty;

            let bob_account_cap = test_scenario::take_from_address<AccountCap>(&test, bob);
            let bob_account_cap_user = object::id(&bob_account_cap);
            let clock = test_scenario::take_shared<Clock>(&test);

            let pool = test_scenario::take_shared<Pool<SUI, USD>>(&mut test);
            let (_, _, bids, _) = get_pool_stat(&pool);

            let orders_expected = empty<Order>();
            push_back(&mut orders_expected, test_construct_order_with_expiration(
                2,
                11 * FLOAT_SCALING,
                2,
                true,
                bob_account_cap_user,
                clock::timestamp_ms(&clock) + 1000000,
            ));
            check_tick_level(bids, 11 * FLOAT_SCALING, &orders_expected);

            test_scenario::return_shared(pool);
            test_scenario::return_shared(clock);
            test_scenario::return_to_address<AccountCap>(bob, bob_account_cap);
        };

        test_scenario::end(test);
    }
}

#[test_only]
/// Tests for the pool module.
/// They are sequential and based on top of each other.

module deepbook::clob_test {
    use std::vector;

    use sui::clock::Clock;
    use sui::coin::{Self, mint_for_testing, burn_for_testing};
    use sui::object;
    use sui::sui::SUI;
    use sui::test_scenario::{Self as test, Scenario, next_tx, ctx, end, TransactionEffects};

    use deepbook::clob::{Self, Pool, Order, USD, account_balance, get_pool_stat, order_id, list_open_orders, mint_account_cap_transfer};
    use deepbook::custodian::{Self, AccountCap};

    const MIN_PRICE: u64 = 0;
    const MAX_PRICE: u64 = ((1u128 << 64 - 1) as u64);
    const MIN_BID_ORDER_ID: u64 = 0;
    const MIN_ASK_ORDER_ID: u64 = 1 << 63;
    const FLOAT_SCALING: u64 = 1000000000;
    const TIMESTAMP_INF: u64 = ((1u128 << 64 - 1) as u64);
    const IMMEDIATE_OR_CANCEL: u8 = 1;
    const FILL_OR_KILL: u8 = 2;
    const POST_OR_ABORT: u8 = 3;
    const E_ORDER_CANNOT_BE_FULLY_FILLED: u64 = 9;

    #[test] fun test_full_transaction() { let _ = test_full_transaction_(scenario()); }

    #[test] fun test_deposit_withdraw() { let _ = test_deposit_withdraw_(scenario()); }

    #[test] fun test_inject_and_match_taker_bid_with_quote_quantity(
    ) { let _ = test_inject_and_match_taker_bid_with_quote_quantity_(scenario()); }

    #[test] fun test_inject_and_match_taker_bid() { let _ = test_inject_and_match_taker_bid_(scenario()); }

    #[test] fun test_inject_and_match_taker_ask() { let _ = test_inject_and_match_taker_ask_(scenario()); }

    #[test] fun test_inject_and_match_taker_bid_with_expiration(
    ) { let _ = test_inject_and_match_taker_bid_with_expiration_(scenario()); }

    #[test] fun test_inject_and_match_taker_bid_with_quote_quantity_and_expiration(
    ) { let _ = test_inject_and_match_taker_bid_with_quote_quantity_and_expiration_(scenario()); }

    #[test] fun test_inject_and_match_taker_ask_with_expiration(
    ) { let _ = test_inject_and_match_taker_ask_with_expiration_(scenario()); }

    #[test] fun test_inject_and_price_limit_affected_match_taker_bid() {
        let _ = test_inject_and_price_limit_affected_match_taker_bid_(
            scenario()
        );
    }

    #[test] fun test_inject_and_price_limit_affected_match_taker_ask() {
        let _ = test_inject_and_price_limit_affected_match_taker_ask_(
            scenario()
        );
    }

    #[test] fun test_remove_order() { let _ = test_remove_order_(scenario()); }


    #[test] fun test_remove_all_orders() { let _ = test_remove_all_orders_(scenario()); }


    #[test] fun test_cancel_and_remove() { let _ = test_cancel_and_remove_(scenario()); }

    #[test] fun test_batch_cancel() { let _ = test_batch_cancel_(scenario()); }

    #[test] fun test_partial_fill_and_cancel() { let _ = test_partial_fill_and_cancel_(scenario()); }

    #[test] fun test_list_open_orders() {
        let _ = test_list_open_orders_(
            scenario()
        );
    }

    #[test] fun get_level2_book_status_bid_side() {
        let _ = get_level2_book_status_bid_side_(
            scenario()
        );
    }

    #[test] fun get_level2_book_status_ask_side() {
        let _ = get_level2_book_status_ask_side_(
            scenario()
        );
    }

    fun get_level2_book_status_bid_side_(test: Scenario): TransactionEffects {
        let (alice, bob) = people();
        // setup pool and custodian
        let owner = @0xF;
        // reset pool and custodian
        next_tx(&mut test, owner);
        {
            clob::setup_test(5000000, 2500000, &mut test, owner);
            mint_account_cap_transfer(alice, test::ctx(&mut test));
            mint_account_cap_transfer(bob, test::ctx(&mut test));
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_mut_custodian(&mut pool);
            let alice_deposit_WSUI: u64 = 100000;
            let alice_deposit_USDC: u64 = 100000;
            custodian::test_increase_user_available_balance<SUI>(base_custodian, account_cap_user, alice_deposit_WSUI);
            custodian::test_increase_user_available_balance<USD>(quote_custodian, account_cap_user, alice_deposit_USDC);
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 500, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 500, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 4 * FLOAT_SCALING, 500, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 4 * FLOAT_SCALING, 500, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 3 * FLOAT_SCALING, 1000, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 2 * FLOAT_SCALING, 1000, true, &account_cap, ctx(&mut test));
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        // test list open orders before match
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let clock = test::take_shared<Clock>(&mut test);
            let (prices, depth) = clob::get_level2_book_status_bid_side(
                &pool,
                1 * FLOAT_SCALING,
                15 * FLOAT_SCALING,
                &clock
            );
            let prices_cmp = vector<u64>[2 * FLOAT_SCALING, 3 * FLOAT_SCALING, 4 * FLOAT_SCALING, 5 * FLOAT_SCALING];
            let depth_cmp = vector<u64>[1000, 1000, 1000, 1000];
            assert!(prices == prices_cmp, 0);
            assert!(depth == depth_cmp, 0);
            test::return_shared(clock);
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        end(test)
    }

    fun get_level2_book_status_ask_side_(test: Scenario): TransactionEffects {
        let (alice, bob) = people();
        // setup pool and custodian
        let owner = @0xF;
        // reset pool and custodian
        next_tx(&mut test, owner);
        {
            clob::setup_test(5000000, 2500000, &mut test, owner);
            mint_account_cap_transfer(alice, test::ctx(&mut test));
            mint_account_cap_transfer(bob, test::ctx(&mut test));
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_mut_custodian(&mut pool);
            let alice_deposit_WSUI: u64 = 100000;
            let alice_deposit_USDC: u64 = 100000;
            custodian::test_increase_user_available_balance<SUI>(base_custodian, account_cap_user, alice_deposit_WSUI);
            custodian::test_increase_user_available_balance<USD>(quote_custodian, account_cap_user, alice_deposit_USDC);
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 500, false, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 500, false, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 4 * FLOAT_SCALING, 500, false, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 4 * FLOAT_SCALING, 500, false, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 3 * FLOAT_SCALING, 1000, false, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 2 * FLOAT_SCALING, 1000, false, &account_cap, ctx(&mut test));
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        // test list open orders before match
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let clock = test::take_shared<Clock>(&mut test);
            let order = clob::get_order_status(&pool, order_id(0, false), &account_cap);
            let order_cmp = clob::test_construct_order(0, 5 * FLOAT_SCALING, 500, false, account_cap_user);
            assert!(order == &order_cmp, 0);
            let (prices, depth) = clob::get_level2_book_status_ask_side(
                &pool,
                1 * FLOAT_SCALING,
                10 * FLOAT_SCALING,
                &clock
            );
            let prices_cmp = vector<u64>[2 * FLOAT_SCALING, 3 * FLOAT_SCALING, 4 * FLOAT_SCALING, 5 * FLOAT_SCALING];
            let depth_cmp = vector<u64>[1000, 1000, 1000, 1000];
            assert!(prices == prices_cmp, 0);
            assert!(depth == depth_cmp, 0);
            test::return_shared(clock);
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        end(test)
    }

    fun test_list_open_orders_(test: Scenario): TransactionEffects {
        let (alice, bob) = people();
        // setup pool and custodian
        let owner = @0xF;
        // reset pool and custodian
        next_tx(&mut test, owner);
        {
            clob::setup_test(5000000, 2500000, &mut test, owner);
            mint_account_cap_transfer(alice, test::ctx(&mut test));
            mint_account_cap_transfer(bob, test::ctx(&mut test));
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_mut_custodian(&mut pool);
            let alice_deposit_WSUI: u64 = 10000;
            let alice_deposit_USDC: u64 = 10000;
            custodian::test_increase_user_available_balance<SUI>(base_custodian, account_cap_user, alice_deposit_WSUI);
            custodian::test_increase_user_available_balance<USD>(quote_custodian, account_cap_user, alice_deposit_USDC);
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 500, false, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 500, false, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 2 * FLOAT_SCALING, 1000, false, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 1 * FLOAT_SCALING, 10000, true, &account_cap, ctx(&mut test));
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        // test list open orders before match
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let open_orders = list_open_orders(&pool, &account_cap);
            let open_orders_cmp = vector::empty<Order>();
            vector::push_back(
                &mut open_orders_cmp,
                clob::test_construct_order(0, 5 * FLOAT_SCALING, 500, false, account_cap_user)
            );
            vector::push_back(
                &mut open_orders_cmp,
                clob::test_construct_order(1, 5 * FLOAT_SCALING, 500, false, account_cap_user)
            );
            vector::push_back(
                &mut open_orders_cmp,
                clob::test_construct_order(2, 2 * FLOAT_SCALING, 1000, false, account_cap_user)
            );
            vector::push_back(
                &mut open_orders_cmp,
                clob::test_construct_order(0, 1 * FLOAT_SCALING, 10000, true, account_cap_user)
            );
            assert!(open_orders == open_orders_cmp, 0);
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };

        // test match (bid side)
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let (base_quantity_filled, quote_quantity_filled) = clob::test_match_bid(
                &mut pool,
                1500,
                MAX_PRICE,
                0,
            );
            assert!(base_quantity_filled == 1500 - 5 - 3, 0);
            // 4500 + 2, 2 from round up
            assert!(quote_quantity_filled == 4500, 0);
            test::return_shared(pool);
        };

        // test list open orders after match
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let open_orders = list_open_orders(&pool, &account_cap);
            let open_orders_cmp = vector::empty<Order>();
            vector::push_back(
                &mut open_orders_cmp,
                clob::test_construct_order(1, 5 * FLOAT_SCALING, 500, false, account_cap_user)
            );
            vector::push_back(
                &mut open_orders_cmp,
                clob::test_construct_order(0, 1 * FLOAT_SCALING, 10000, true, account_cap_user)
            );
            assert!(open_orders == open_orders_cmp, 0);
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };

        // reset pool and custodian
        next_tx(&mut test, owner);
        {
            clob::setup_test(5000000, 2500000, &mut test, owner);
            mint_account_cap_transfer(alice, test::ctx(&mut test));
            mint_account_cap_transfer(bob, test::ctx(&mut test));
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_mut_custodian(&mut pool);
            let alice_deposit_WSUI: u64 = 10000;
            let alice_deposit_USDC: u64 = 10000;
            custodian::test_increase_user_available_balance<SUI>(base_custodian, account_cap_user, alice_deposit_WSUI);
            custodian::test_increase_user_available_balance<USD>(quote_custodian, account_cap_user, alice_deposit_USDC);
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 500, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 500, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 2 * FLOAT_SCALING, 1000, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 10 * FLOAT_SCALING, 10000, false, &account_cap, ctx(&mut test));
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        // test list open orders before match
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let open_orders = list_open_orders(&pool, &account_cap);
            let open_orders_cmp = vector::empty<Order>();
            vector::push_back(
                &mut open_orders_cmp,
                clob::test_construct_order(0, 5 * FLOAT_SCALING, 500, true, account_cap_user)
            );
            vector::push_back(
                &mut open_orders_cmp,
                clob::test_construct_order(1, 5 * FLOAT_SCALING, 500, true, account_cap_user)
            );
            vector::push_back(
                &mut open_orders_cmp,
                clob::test_construct_order(2, 2 * FLOAT_SCALING, 1000, true, account_cap_user)
            );
            vector::push_back(
                &mut open_orders_cmp,
                clob::test_construct_order(0, 10 * FLOAT_SCALING, 10000, false, account_cap_user)
            );
            assert!(open_orders == open_orders_cmp, 0);
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };

        // test match (ask side)
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let (base_quantity_filled, quote_quantity_filled) = clob::test_match_ask(
                &mut pool,
                1500,
                MIN_PRICE,
                0,
            );
            assert!(base_quantity_filled == 1500, 0);
            assert!(quote_quantity_filled == 6000 - 13 - 13 - 5, 0);
            test::return_shared(pool);
        };

        // test list open orders after match
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let open_orders = list_open_orders(&pool, &account_cap);
            let open_orders_cmp = vector::empty<Order>();
            vector::push_back(
                &mut open_orders_cmp,
                clob::test_construct_order(2, 2 * FLOAT_SCALING, 500, true, account_cap_user)
            );
            vector::push_back(
                &mut open_orders_cmp,
                clob::test_construct_order(0, 10 * FLOAT_SCALING, 10000, false, account_cap_user)
            );
            assert!(open_orders == open_orders_cmp, 0);
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        end(test)
    }


    fun test_deposit_withdraw_(test: Scenario): TransactionEffects {
        let (alice, _) = people();
        let owner: address = @0xF;
        next_tx(&mut test, owner);
        {
            clob::setup_test(0, 0, &mut test, owner);
            mint_account_cap_transfer(alice, test::ctx(&mut test));
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let alice_deposit_WSUI: u64 = 10000;
            let alice_withdraw_WSUI: u64 = 5000;
            let alice_deposit_USDC: u64 = 10000;
            let alice_withdraw_USDC: u64 = 1000;
            clob::deposit_base(&mut pool, mint_for_testing<SUI>(alice_deposit_WSUI, ctx(&mut test)), &account_cap);
            clob::deposit_quote(&mut pool, mint_for_testing<USD>(alice_deposit_USDC, ctx(&mut test)), &account_cap);
            burn_for_testing(clob::withdraw_base(&mut pool, alice_withdraw_WSUI, &account_cap, ctx(&mut test)));
            burn_for_testing(clob::withdraw_quote(&mut pool, alice_withdraw_USDC, &account_cap, ctx(&mut test)));
            let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);
            custodian::assert_user_balance(
                base_custodian,
                account_cap_user,
                alice_deposit_WSUI - alice_withdraw_WSUI,
                0
            );
            custodian::assert_user_balance(
                quote_custodian,
                account_cap_user,
                alice_deposit_USDC - alice_withdraw_USDC,
                0
            );
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        end(test)
    }

    fun test_batch_cancel_(test: Scenario): TransactionEffects {
        let (alice, _) = people();
        // setup pool and custodian
        let owner: address = @0xF;
        next_tx(&mut test, owner);
        {
            clob::setup_test(0, 0, &mut test, owner);
            mint_account_cap_transfer(alice, test::ctx(&mut test));
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_mut_custodian(&mut pool);
            let alice_deposit_WSUI: u64 = 10000;
            let alice_deposit_USDC: u64 = 10000;
            custodian::test_increase_user_available_balance<SUI>(base_custodian, account_cap_user, alice_deposit_WSUI);
            custodian::test_increase_user_available_balance<USD>(quote_custodian, account_cap_user, alice_deposit_USDC);

            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 500, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 500, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 2 * FLOAT_SCALING, 1000, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 10 * FLOAT_SCALING, 10000, false, &account_cap, ctx(&mut test));
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            // let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);
            // custodian::assert_user_balance<USDC>(quote_custodian, alice, 3000, 7000);
            // custodian::assert_user_balance<WSUI>(base_custodian, alice, 0, 10000);
            let (next_bid_order_id, next_ask_order_id, _, _) = clob::get_pool_stat(&pool);
            assert!(next_bid_order_id == clob::order_id(3, true), 0);
            assert!(next_ask_order_id == clob::order_id(1, false), 0);

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 5 * FLOAT_SCALING, 500, true, account_cap_user)
                );
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(1, 5 * FLOAT_SCALING, 500, true, account_cap_user)
                );
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_tick_level(bids, 5 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(2, 2 * FLOAT_SCALING, 1000, true, account_cap_user)
                );
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_tick_level(bids, 2 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 10 * FLOAT_SCALING, 10000, false, account_cap_user)
                );
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_tick_level(asks, 10 * FLOAT_SCALING, &open_orders);
            };
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };

        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let orders = vector::empty<u64>();
            vector::push_back(&mut orders, 0);
            vector::push_back(&mut orders, 1);
            clob::batch_cancel_order(&mut pool, orders, &account_cap);
            {
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_empty_tick_level(bids, 5 * FLOAT_SCALING);
            };
            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(2, 2 * FLOAT_SCALING, 1000, true, account_cap_user)
                );
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_tick_level(bids, 2 * FLOAT_SCALING, &open_orders);
            };
            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 10 * FLOAT_SCALING, 10000, false, account_cap_user)
                );
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_tick_level(asks, 10 * FLOAT_SCALING, &open_orders);
            };

            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        end(test)
    }

    fun test_partial_fill_and_cancel_(test: Scenario): TransactionEffects {
        let (alice, bob) = people();
        // setup pool and custodian
        let owner: address = @0xF;
        next_tx(&mut test, owner);
        {
            clob::setup_test(0, 0, &mut test, owner);
            mint_account_cap_transfer(alice, test::ctx(&mut test));
            mint_account_cap_transfer(bob, test::ctx(&mut test));
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_sender<AccountCap>(&test);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_mut_custodian(&mut pool);
            custodian::deposit(
                base_custodian,
                mint_for_testing<SUI>(1000 * 100000000, ctx(&mut test)),
                account_cap_user
            );
            custodian::deposit(
                quote_custodian,
                mint_for_testing<USD>(10000 * 100000000, ctx(&mut test)),
                account_cap_user
            );
            test::return_shared(pool);
            test::return_to_sender<AccountCap>(&test, account_cap);
        };

        // alice place series limit order
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_sender<AccountCap>(&test);
            let clock = test::take_shared<Clock>(&mut test);
            clob::place_limit_order<SUI, USD>(
                &mut pool,
                5 * FLOAT_SCALING,
                200 * 100000000,
                true,
                TIMESTAMP_INF,
                0,
                &clock,
                &account_cap,
                ctx(&mut test)
            );
            clob::place_limit_order<SUI, USD>(
                &mut pool,
                4 * FLOAT_SCALING,
                200 * 100000000,
                true,
                TIMESTAMP_INF,
                0,
                &clock,
                &account_cap,
                ctx(&mut test)
            );
            clob::place_limit_order<SUI, USD>(
                &mut pool,
                4 * FLOAT_SCALING,
                200 * 100000000,
                true,
                TIMESTAMP_INF,
                0,
                &clock,
                &account_cap,
                ctx(&mut test)
            );
            clob::place_limit_order<SUI, USD>(
                &mut pool,
                10 * FLOAT_SCALING,
                1000 * 100000000,
                false,
                TIMESTAMP_INF,
                0,
                &clock,
                &account_cap,
                ctx(&mut test)
            );
            let (next_bid_order_id, next_ask_order_id, _, _) = clob::get_pool_stat(&pool);
            assert!(next_bid_order_id == clob::order_id(3, true), 0);
            assert!(next_ask_order_id == clob::order_id(1, false), 0);
            test::return_shared(clock);
            test::return_shared(pool);
            test::return_to_sender<AccountCap>(&test, account_cap);
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_sender<AccountCap>(&test);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);
            custodian::assert_user_balance<USD>(quote_custodian, account_cap_user, 7400 * 100000000, 2600 * 100000000);
            custodian::assert_user_balance<SUI>(base_custodian, account_cap_user, 0, 1000 * 100000000);
            test::return_shared(pool);
            test::return_to_sender<AccountCap>(&test, account_cap);
        };
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, bob);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, _) = clob::borrow_mut_custodian(&mut pool);
            custodian::deposit(
                base_custodian,
                mint_for_testing<SUI>(300 * 100000000, ctx(&mut test)),
                account_cap_user
            );
            custodian::assert_user_balance<SUI>(base_custodian, account_cap_user, 300 * 100000000, 0);
            test::return_shared(pool);
            test::return_to_address<AccountCap>(bob, account_cap);
        };

        // bob palce series market order
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_sender<AccountCap>(&test);
            let clock = test::take_shared<Clock>(&mut test);
            clob::place_limit_order<SUI, USD>(
                &mut pool,
                4 * FLOAT_SCALING,
                300 * 100000000,
                false,
                TIMESTAMP_INF,
                0,
                &clock,
                &account_cap,
                ctx(&mut test)
            );
            test::return_shared(clock);
            test::return_shared(pool);
            test::return_to_sender<AccountCap>(&test, account_cap);
        };
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, bob);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_mut_custodian(&mut pool);
            custodian::assert_user_balance<SUI>(base_custodian, account_cap_user, 0, 0);
            custodian::assert_user_balance<USD>(quote_custodian, account_cap_user, 1400 * 100000000, 0);
            test::return_shared(pool);
            test::return_to_address<AccountCap>(bob, account_cap);
        };

        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            {
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_empty_tick_level(bids, 5 * FLOAT_SCALING);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(1, 4 * FLOAT_SCALING, 100 * 100000000, true, account_cap_user)
                );
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(2, 4 * FLOAT_SCALING, 200 * 100000000, true, account_cap_user)
                );
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_tick_level(bids, 4 * FLOAT_SCALING, &open_orders);
            };

            clob::cancel_order<SUI, USD>(&mut pool, 1, &account_cap);
            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(2, 4 * FLOAT_SCALING, 200 * 100000000, true, account_cap_user)
                );
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_tick_level(bids, 4 * FLOAT_SCALING, &open_orders);
            };

            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };

        end(test)
    }

    fun test_full_transaction_(test: Scenario): TransactionEffects {
        let (alice, bob) = people();
        // setup pool and custodian
        let owner: address = @0xF;
        next_tx(&mut test, owner);
        {
            clob::setup_test(5000000, 2500000, &mut test, owner);
            mint_account_cap_transfer(alice, test::ctx(&mut test));
        };

        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_sender<AccountCap>(&test);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_mut_custodian(&mut pool);
            custodian::deposit(base_custodian, mint_for_testing<SUI>(1000, ctx(&mut test)), account_cap_user);
            custodian::deposit(quote_custodian, mint_for_testing<USD>(10000, ctx(&mut test)), account_cap_user);
            test::return_shared(pool);
            test::return_to_sender<AccountCap>(&test, account_cap);
        };

        // alice palce series limit order
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let clock = test::take_shared<Clock>(&mut test);
            clob::place_limit_order<SUI, USD>(
                &mut pool,
                5 * FLOAT_SCALING,
                200,
                true,
                TIMESTAMP_INF,
                0,
                &clock,
                &account_cap,
                ctx(&mut test)
            );
            clob::place_limit_order<SUI, USD>(
                &mut pool,
                5 * FLOAT_SCALING,
                300,
                true,
                TIMESTAMP_INF,
                0,
                &clock,
                &account_cap,
                ctx(&mut test)
            );
            clob::place_limit_order<SUI, USD>(
                &mut pool,
                2 * FLOAT_SCALING,
                1000,
                true,
                TIMESTAMP_INF,
                0,
                &clock,
                &account_cap,
                ctx(&mut test)
            );
            clob::place_limit_order<SUI, USD>(
                &mut pool,
                20 * FLOAT_SCALING,
                1000,
                false,
                TIMESTAMP_INF,
                0,
                &clock,
                &account_cap,
                ctx(&mut test)
            );
            let (next_bid_order_id, next_ask_order_id, _, _) = clob::get_pool_stat(&pool);
            assert!(next_bid_order_id == clob::order_id(3, true), 0);
            assert!(next_ask_order_id == clob::order_id(1, false), 0);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);
            custodian::assert_user_balance(base_custodian, account_cap_user, 0, 1000);
            custodian::assert_user_balance(quote_custodian, account_cap_user, 5500, 4500);
            let (base_avail, base_locked, quote_avail, quote_locked) = account_balance(&pool, &account_cap);
            assert!(base_avail == 0, 0);
            assert!(base_locked == 1000, 0);
            assert!(quote_avail == 5500, 0);
            assert!(quote_locked == 4500, 0);
            test::return_shared(pool);
            test::return_shared(clock);
            test::return_to_address<AccountCap>(alice, account_cap);
        };

        // bob palce series market order
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let clock = test::take_shared<Clock>(&mut test);
            let (coin1, coin2) = clob::place_market_order<SUI, USD>(&mut pool, 600, false,
                mint_for_testing<SUI>(600, ctx(&mut test)),
                mint_for_testing<USD>(0, ctx(&mut test)),
                &clock,
                ctx(&mut test));
            assert!(coin::value<SUI>(&coin1) == 0, 0);
            assert!(coin::value<USD>(&coin2) == 2700 - 14, 0);
            burn_for_testing(coin1);
            burn_for_testing(coin2);
            test::return_shared(pool);
            test::return_shared(clock);
        };
        end(test)
    }

    fun test_cancel_and_remove_(test: Scenario): TransactionEffects {
        let (alice, _) = people();
        let owner: address = @0xF;
        next_tx(&mut test, owner);
        {
            clob::setup_test(0, 0, &mut test, owner);
            mint_account_cap_transfer(alice, test::ctx(&mut test));
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_mut_custodian(&mut pool);
            let alice_deposit_WSUI: u64 = 10;
            let alice_deposit_USDC: u64 = 100;
            custodian::test_increase_user_available_balance<SUI>(base_custodian, account_cap_user, alice_deposit_WSUI);
            custodian::test_increase_user_available_balance<USD>(quote_custodian, account_cap_user, alice_deposit_USDC);

            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 2, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 3, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 2 * FLOAT_SCALING, 10, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 2 * FLOAT_SCALING, 10, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 20 * FLOAT_SCALING, 10, false, &account_cap, ctx(&mut test));
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);
            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 5 * FLOAT_SCALING, 2, true, account_cap_user)
                );
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(1, 5 * FLOAT_SCALING, 3, true, account_cap_user)
                );
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_tick_level(bids, 5 * FLOAT_SCALING, &open_orders);
            };
            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(2, 2 * FLOAT_SCALING, 10, true, account_cap_user)
                );
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(3, 2 * FLOAT_SCALING, 10, true, account_cap_user)
                );
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_tick_level(bids, 2 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 20 * FLOAT_SCALING, 10, false, account_cap_user)
                );
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_tick_level(asks, 20 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 5 * FLOAT_SCALING, 2, true, account_cap_user)
                );
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(1, 5 * FLOAT_SCALING, 3, true, account_cap_user)
                );
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_tick_level(bids, 5 * FLOAT_SCALING, &open_orders);
            };

            custodian::assert_user_balance<USD>(quote_custodian, account_cap_user, 35, 65);
            custodian::assert_user_balance<SUI>(base_custodian, account_cap_user, 0, 10);

            // check usr open orders before cancel
            {
                let usr_open_orders_cmp = vector::empty<u64>();
                vector::push_back(&mut usr_open_orders_cmp, order_id(0, true));
                vector::push_back(&mut usr_open_orders_cmp, 5 * FLOAT_SCALING);
                vector::push_back(&mut usr_open_orders_cmp, order_id(1, true));
                vector::push_back(&mut usr_open_orders_cmp, 5 * FLOAT_SCALING);
                vector::push_back(&mut usr_open_orders_cmp, order_id(2, true));
                vector::push_back(&mut usr_open_orders_cmp, 2 * FLOAT_SCALING);
                vector::push_back(&mut usr_open_orders_cmp, order_id(3, true));
                vector::push_back(&mut usr_open_orders_cmp, 2 * FLOAT_SCALING);
                vector::push_back(&mut usr_open_orders_cmp, order_id(0, false));
                vector::push_back(&mut usr_open_orders_cmp, 20 * FLOAT_SCALING);
                clob::check_usr_open_orders(clob::get_usr_open_orders(&pool, account_cap_user), &usr_open_orders_cmp);
            };
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };

        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            clob::cancel_order(&mut pool, 0, &account_cap);
            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(1, 5 * FLOAT_SCALING, 3, true, account_cap_user)
                );
                // check tick level from pool after remove
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_tick_level(bids, 5 * FLOAT_SCALING, &open_orders);
                // check usr open orders after remove order bid-0
                let usr_open_orders_cmp = vector::empty<u64>();
                vector::push_back(&mut usr_open_orders_cmp, order_id(0, false));
                vector::push_back(&mut usr_open_orders_cmp, 20 * FLOAT_SCALING);
                vector::push_back(&mut usr_open_orders_cmp, order_id(1, true));
                vector::push_back(&mut usr_open_orders_cmp, 5 * FLOAT_SCALING);
                vector::push_back(&mut usr_open_orders_cmp, order_id(2, true));
                vector::push_back(&mut usr_open_orders_cmp, 2 * FLOAT_SCALING);
                vector::push_back(&mut usr_open_orders_cmp, order_id(3, true));
                vector::push_back(&mut usr_open_orders_cmp, 2 * FLOAT_SCALING);
                clob::check_usr_open_orders(clob::get_usr_open_orders(&pool, account_cap_user), &usr_open_orders_cmp);
            };
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };

        next_tx(&mut test, alice);
        {
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);
            custodian::assert_user_balance<USD>(quote_custodian, account_cap_user, 35 + 10, 65 - 10);
            custodian::assert_user_balance<SUI>(base_custodian, account_cap_user, 0, 10);
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            clob::cancel_order(&mut pool, 1, &account_cap);
            {
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_empty_tick_level(bids, 5 * FLOAT_SCALING);
                let usr_open_orders_cmp = vector::empty<u64>();
                vector::push_back(&mut usr_open_orders_cmp, order_id(0, false));
                vector::push_back(&mut usr_open_orders_cmp, 20 * FLOAT_SCALING);
                vector::push_back(&mut usr_open_orders_cmp, order_id(3, true));
                vector::push_back(&mut usr_open_orders_cmp, 2 * FLOAT_SCALING);
                vector::push_back(&mut usr_open_orders_cmp, order_id(2, true));
                vector::push_back(&mut usr_open_orders_cmp, 2 * FLOAT_SCALING);
                clob::check_usr_open_orders(clob::get_usr_open_orders(&pool, account_cap_user), &usr_open_orders_cmp);
            };
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);
            custodian::assert_user_balance<USD>(quote_custodian, account_cap_user, 35 + 10 + 15, 65 - 10 - 15);
            custodian::assert_user_balance<SUI>(base_custodian, account_cap_user, 0, 10);
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            clob::cancel_order(&mut pool, MIN_ASK_ORDER_ID, &account_cap);
            {
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_empty_tick_level(asks, 20 * FLOAT_SCALING);
                let usr_open_orders_cmp = vector::empty<u64>();
                vector::push_back(&mut usr_open_orders_cmp, order_id(3, true));
                vector::push_back(&mut usr_open_orders_cmp, 2 * FLOAT_SCALING);
                vector::push_back(&mut usr_open_orders_cmp, order_id(2, true));
                vector::push_back(&mut usr_open_orders_cmp, 2 * FLOAT_SCALING);
                clob::check_usr_open_orders(clob::get_usr_open_orders(&pool, account_cap_user), &usr_open_orders_cmp);
            };
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);
            custodian::assert_user_balance<USD>(quote_custodian, account_cap_user, 35 + 10 + 15, 65 - 10 - 15);
            custodian::assert_user_balance<SUI>(base_custodian, account_cap_user, 10, 0);
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        end(test)
    }

    fun test_inject_and_match_taker_bid_with_quote_quantity_(test: Scenario): TransactionEffects {
        let (alice, bob) = people();
        // setup pool and custodian
        let owner = @0xF;
        // reset pool and custodian
        next_tx(&mut test, owner);
        {
            clob::setup_test(5000000, 2500000, &mut test, owner);
            mint_account_cap_transfer(alice, test::ctx(&mut test));
            mint_account_cap_transfer(bob, test::ctx(&mut test));
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_mut_custodian(&mut pool);
            let alice_deposit_WSUI: u64 = 10000;
            let alice_deposit_USDC: u64 = 10000;
            custodian::test_increase_user_available_balance<SUI>(base_custodian, account_cap_user, alice_deposit_WSUI);
            custodian::test_increase_user_available_balance<USD>(quote_custodian, account_cap_user, alice_deposit_USDC);
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 500, false, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 500, false, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 2 * FLOAT_SCALING, 1000, false, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 1 * FLOAT_SCALING, 10000, true, &account_cap, ctx(&mut test));
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };

        // test inject limit order and match (bid side)
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);

            custodian::assert_user_balance<USD>(quote_custodian, account_cap_user, 0, 10000);
            custodian::assert_user_balance<SUI>(base_custodian, account_cap_user, 8000, 2000);
            let (next_bid_order_id, next_ask_order_id, _, _) = clob::get_pool_stat(&pool);
            assert!(next_bid_order_id == clob::order_id(1, true), 0);
            assert!(next_ask_order_id == clob::order_id(3, false), 0);

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 5 * FLOAT_SCALING, 500, false, account_cap_user)
                );
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(1, 5 * FLOAT_SCALING, 500, false, account_cap_user)
                );
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_tick_level(asks, 5 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(2, 2 * FLOAT_SCALING, 1000, false, account_cap_user)
                );
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_tick_level(asks, 2 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 1 * FLOAT_SCALING, 10000, true, account_cap_user)
                );
                let (_, _, bid, _) = get_pool_stat(&pool);
                clob::check_tick_level(bid, 1 * FLOAT_SCALING, &open_orders);
            };
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };

        // test match (bid side)
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let (base_quantity_filled, quote_quantity_filled) = clob::test_match_bid_with_quote_quantity(
                &mut pool,
                4500,
                MAX_PRICE,
                0,
            );
            assert!(base_quantity_filled == 1492, 0);
            assert!(quote_quantity_filled == 4500, 0);
            test::return_shared(pool);
        };
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap_alice = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user_alice = object::id(&account_cap_alice);
            let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);

            // custodian before match
            // custodian::assert_user_balance<USDC>(&custodian, alice, 0, 10000);
            // custodian::assert_user_balance<WSUI>(&custodian, alice, 8000, 2000);
            // rebate fee in base asset 3
            custodian::assert_user_balance<USD>(quote_custodian, account_cap_user_alice, 4500, 10000);
            custodian::assert_user_balance<SUI>(base_custodian, account_cap_user_alice, 8000 + 3, 500);
            {
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_empty_tick_level(asks, 2 * FLOAT_SCALING);
            };
            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(1, 5 * FLOAT_SCALING, 500, false, account_cap_user_alice)
                );
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_tick_level(asks, 5 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 1 * FLOAT_SCALING, 10000, true, account_cap_user_alice)
                );
                let (_, _, bid, _) = get_pool_stat(&pool);
                clob::check_tick_level(bid, 1 * FLOAT_SCALING, &open_orders);
            };

            let open_orders = list_open_orders(&pool, &account_cap_alice);
            let open_orders_cmp = vector::empty<Order>();
            vector::push_back(
                &mut open_orders_cmp,
                clob::test_construct_order(1, 5 * FLOAT_SCALING, 500, false, account_cap_user_alice)
            );
            vector::push_back(
                &mut open_orders_cmp,
                clob::test_construct_order(0, 1 * FLOAT_SCALING, 10000, true, account_cap_user_alice)
            );
            assert!(open_orders == open_orders_cmp, 0);
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap_alice);
        };
        end(test)
    }

    fun test_inject_and_match_taker_bid_(test: Scenario): TransactionEffects {
        let (alice, bob) = people();
        // setup pool and custodian
        let owner = @0xF;
        // reset pool and custodian
        next_tx(&mut test, owner);
        {
            clob::setup_test(5000000, 2500000, &mut test, owner);
            mint_account_cap_transfer(alice, test::ctx(&mut test));
            mint_account_cap_transfer(bob, test::ctx(&mut test));
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_mut_custodian(&mut pool);
            let alice_deposit_WSUI: u64 = 10000;
            let alice_deposit_USDC: u64 = 10000;
            custodian::test_increase_user_available_balance<SUI>(base_custodian, account_cap_user, alice_deposit_WSUI);
            custodian::test_increase_user_available_balance<USD>(quote_custodian, account_cap_user, alice_deposit_USDC);
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 500, false, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 500, false, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 2 * FLOAT_SCALING, 1000, false, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 1 * FLOAT_SCALING, 10000, true, &account_cap, ctx(&mut test));
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };

        // test inject limit order and match (bid side)
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);
            custodian::assert_user_balance<USD>(quote_custodian, account_cap_user, 0, 10000);
            custodian::assert_user_balance<SUI>(base_custodian, account_cap_user, 8000, 2000);
            let (next_bid_order_id, next_ask_order_id, _, _) = clob::get_pool_stat(&pool);
            assert!(next_bid_order_id == clob::order_id(1, true), 0);
            assert!(next_ask_order_id == clob::order_id(3, false), 0);
            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 5 * FLOAT_SCALING, 500, false, account_cap_user)
                );
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(1, 5 * FLOAT_SCALING, 500, false, account_cap_user)
                );
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_tick_level(asks, 5 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(2, 2 * FLOAT_SCALING, 1000, false, account_cap_user)
                );
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_tick_level(asks, 2 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 1 * FLOAT_SCALING, 10000, true, account_cap_user)
                );
                let (_, _, bid, _) = get_pool_stat(&pool);
                clob::check_tick_level(bid, 1 * FLOAT_SCALING, &open_orders);
            };
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };

        // test match (bid side)
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let (base_quantity_filled, quote_quantity_filled) = clob::test_match_bid(
                &mut pool,
                1500,
                MAX_PRICE,
                0,
            );
            assert!(base_quantity_filled == 1500 - 5 - 3, 0);
            // 4500 + 2, 2 from round up
            assert!(quote_quantity_filled == 4500, 0);
            test::return_shared(pool);
        };
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap_alice = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user_alice = object::id(&account_cap_alice);
            let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);

            // custodian before match
            // custodian::assert_user_balance<USDC>(&custodian, alice, 0, 10000);
            // custodian::assert_user_balance<WSUI>(&custodian, alice, 8000, 2000);
            // rebate fee in base asset 3
            custodian::assert_user_balance<USD>(quote_custodian, account_cap_user_alice, 4500, 10000);
            custodian::assert_user_balance<SUI>(base_custodian, account_cap_user_alice, 8000 + 3, 500);
            {
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_empty_tick_level(asks, 2 * FLOAT_SCALING);
            };
            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(1, 5 * FLOAT_SCALING, 500, false, account_cap_user_alice)
                );
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_tick_level(asks, 5 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 1 * FLOAT_SCALING, 10000, true, account_cap_user_alice)
                );
                let (_, _, bid, _) = get_pool_stat(&pool);
                clob::check_tick_level(bid, 1 * FLOAT_SCALING, &open_orders);
            };
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap_alice);
        };
        end(test)
    }

    fun test_inject_and_match_taker_ask_(test: Scenario): TransactionEffects {
        let (alice, bob) = people();
        // setup pool and custodian
        let owner = @0xF;
        next_tx(&mut test, owner);
        {
            // taker_fee_rate = 0.005; maker_rebate_fee = 0.0025;
            clob::setup_test(5000000, 2500000, &mut test, owner);
            mint_account_cap_transfer(alice, test::ctx(&mut test));
            mint_account_cap_transfer(bob, test::ctx(&mut test));
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_mut_custodian(&mut pool);
            let alice_deposit_WSUI: u64 = 10000;
            let alice_deposit_USDC: u64 = 10000;
            custodian::test_increase_user_available_balance<SUI>(base_custodian, account_cap_user, alice_deposit_WSUI);
            custodian::test_increase_user_available_balance<USD>(quote_custodian, account_cap_user, alice_deposit_USDC);
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        // test inject limit order (bid side) and match
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            // let account_cap_user = get_account_cap_user(&account_cap);
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 500, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 500, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 2 * FLOAT_SCALING, 1000, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 10 * FLOAT_SCALING, 10000, false, &account_cap, ctx(&mut test));
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };

        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);
            custodian::assert_user_balance<USD>(quote_custodian, account_cap_user, 3000, 7000);
            custodian::assert_user_balance<SUI>(base_custodian, account_cap_user, 0, 10000);
            let (next_bid_order_id, next_ask_order_id, _, _) = clob::get_pool_stat(&pool);
            assert!(next_bid_order_id == clob::order_id(3, true), 0);
            assert!(next_ask_order_id == clob::order_id(1, false), 0);

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 5 * FLOAT_SCALING, 500, true, account_cap_user)
                );
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(1, 5 * FLOAT_SCALING, 500, true, account_cap_user)
                );
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_tick_level(bids, 5 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(2, 2 * FLOAT_SCALING, 1000, true, account_cap_user)
                );
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_tick_level(bids, 2 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 10 * FLOAT_SCALING, 10000, false, account_cap_user)
                );
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_tick_level(asks, 10 * FLOAT_SCALING, &open_orders);
            };
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };

        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let (base_quantity_filled, quote_quantity_filled) = clob::test_match_ask(
                &mut pool,
                1500,
                MIN_PRICE,
                0,
            );
            assert!(base_quantity_filled == 1500, 0);
            assert!(quote_quantity_filled == 6000 - 13 - 13 - 5, 0);
            test::return_shared(pool);
        };

        // test match (ask side)
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);
            // before match
            // custodian::assert_user_balance<USDC>(&custodian, alice, 3000, 7000);
            // custodian::assert_user_balance<WSUI>(&custodian, alice, 0, 10000);
            // rebate
            custodian::assert_user_balance<USD>(
                quote_custodian,
                account_cap_user,
                3000 + 6 + 6 + 2,
                7000 - 2500 - 2500 - 1000
            );
            custodian::assert_user_balance<SUI>(base_custodian, account_cap_user, 1500, 10000);
            {
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_empty_tick_level(bids, 5 * FLOAT_SCALING);
            };
            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(2, 2 * FLOAT_SCALING, 500, true, account_cap_user)
                );
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_tick_level(bids, 2 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 10 * FLOAT_SCALING, 10000, false, account_cap_user)
                );
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_tick_level(asks, 10 * FLOAT_SCALING, &open_orders);
            };
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        end(test)
    }

    fun test_inject_and_match_taker_bid_with_quote_quantity_and_expiration_(test: Scenario): TransactionEffects {
        let (alice, bob) = people();
        // setup pool and custodian
        let owner = @0xF;
        // reset pool and custodian
        next_tx(&mut test, owner);
        {
            // taker_fee_rate = 0.005; maker_rebate_fee = 0.0025;
            clob::setup_test(5000000, 2500000, &mut test, owner);
            mint_account_cap_transfer(alice, test::ctx(&mut test));
            mint_account_cap_transfer(bob, test::ctx(&mut test));
        };
        // test inject limit order and match (bid side)
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_mut_custodian(&mut pool);
            let alice_deposit_WSUI: u64 = 10000;
            let alice_deposit_USDC: u64 = 10000;
            custodian::test_increase_user_available_balance<SUI>(base_custodian, account_cap_user, alice_deposit_WSUI);
            custodian::test_increase_user_available_balance<USD>(quote_custodian, account_cap_user, alice_deposit_USDC);
            clob::test_inject_limit_order_with_expiration(
                &mut pool,
                5 * FLOAT_SCALING,
                500,
                false,
                TIMESTAMP_INF,
                &account_cap,
                ctx(&mut test)
            );
            clob::test_inject_limit_order_with_expiration(
                &mut pool,
                2 * FLOAT_SCALING,
                500,
                false,
                0,
                &account_cap,
                ctx(&mut test)
            );
            clob::test_inject_limit_order_with_expiration(
                &mut pool,
                2 * FLOAT_SCALING,
                1000,
                false,
                TIMESTAMP_INF,
                &account_cap,
                ctx(&mut test)
            );
            clob::test_inject_limit_order_with_expiration(
                &mut pool,
                1 * FLOAT_SCALING,
                10000,
                true,
                0,
                &account_cap,
                ctx(&mut test)
            );
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);

            custodian::assert_user_balance<USD>(quote_custodian, account_cap_user, 0, 10000);
            custodian::assert_user_balance<SUI>(base_custodian, account_cap_user, 8000, 2000);
            let (next_bid_order_id, next_ask_order_id, _, _) = clob::get_pool_stat(&pool);
            assert!(next_bid_order_id == clob::order_id(1, true), 0);
            assert!(next_ask_order_id == clob::order_id(3, false), 0);

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order_with_expiration(
                        0,
                        5 * FLOAT_SCALING,
                        500,
                        false,
                        account_cap_user,
                        TIMESTAMP_INF
                    )
                );
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_tick_level(asks, 5 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order_with_expiration(1, 2 * FLOAT_SCALING, 500, false, account_cap_user, 0)
                );
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order_with_expiration(
                        2,
                        2 * FLOAT_SCALING,
                        1000,
                        false,
                        account_cap_user,
                        TIMESTAMP_INF
                    )
                );
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_tick_level(asks, 2 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 1 * FLOAT_SCALING, 10000, true, account_cap_user)
                );
                let (_, _, bid, _) = get_pool_stat(&pool);
                clob::check_tick_level(bid, 1 * FLOAT_SCALING, &open_orders);
            };
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };

        // test match (bid side)
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let (base_quantity_filled, quote_quantity_filled) = clob::test_match_bid(
                &mut pool,
                4500,
                MAX_PRICE,
                1,
            );
            assert!(base_quantity_filled == 1492, 0);
            // 4500
            assert!(quote_quantity_filled == 4500, 0);
            test::return_shared(pool);
        };
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);
            // rebate fee in base asset 3
            custodian::assert_user_balance<USD>(quote_custodian, account_cap_user, 4500, 10000);
            custodian::assert_user_balance<SUI>(base_custodian, account_cap_user, 8500 + 3, 0);
            {
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_empty_tick_level(asks, 2 * FLOAT_SCALING);
            };
            {
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_empty_tick_level(asks, 5 * FLOAT_SCALING);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 1 * FLOAT_SCALING, 10000, true, account_cap_user)
                );
                let (_, _, bid, _) = get_pool_stat(&pool);
                clob::check_tick_level(bid, 1 * FLOAT_SCALING, &open_orders);
            };
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        end(test)
    }


    fun test_inject_and_match_taker_bid_with_expiration_(test: Scenario): TransactionEffects {
        let (alice, bob) = people();
        // setup pool and custodian
        let owner = @0xF;
        // reset pool and custodian
        next_tx(&mut test, owner);
        {
            // taker_fee_rate = 0.005; maker_rebate_fee = 0.0025;
            clob::setup_test(5000000, 2500000, &mut test, owner);
            mint_account_cap_transfer(alice, test::ctx(&mut test));
            mint_account_cap_transfer(bob, test::ctx(&mut test));
        };
        // test inject limit order and match (bid side)
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_mut_custodian(&mut pool);
            let alice_deposit_WSUI: u64 = 10000;
            let alice_deposit_USDC: u64 = 10000;
            custodian::test_increase_user_available_balance<SUI>(base_custodian, account_cap_user, alice_deposit_WSUI);
            custodian::test_increase_user_available_balance<USD>(quote_custodian, account_cap_user, alice_deposit_USDC);
            clob::test_inject_limit_order_with_expiration(
                &mut pool,
                5 * FLOAT_SCALING,
                500,
                false,
                TIMESTAMP_INF,
                &account_cap,
                ctx(&mut test)
            );
            clob::test_inject_limit_order_with_expiration(
                &mut pool,
                2 * FLOAT_SCALING,
                500,
                false,
                0,
                &account_cap,
                ctx(&mut test)
            );
            clob::test_inject_limit_order_with_expiration(
                &mut pool,
                2 * FLOAT_SCALING,
                1000,
                false,
                TIMESTAMP_INF,
                &account_cap,
                ctx(&mut test)
            );
            clob::test_inject_limit_order_with_expiration(
                &mut pool,
                1 * FLOAT_SCALING,
                10000,
                true,
                0,
                &account_cap,
                ctx(&mut test)
            );
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let clock = test::take_shared<Clock>(&test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);

            custodian::assert_user_balance<USD>(quote_custodian, account_cap_user, 0, 10000);
            custodian::assert_user_balance<SUI>(base_custodian, account_cap_user, 8000, 2000);
            let (next_bid_order_id, next_ask_order_id, _, _) = clob::get_pool_stat(&pool);
            assert!(next_bid_order_id == clob::order_id(1, true), 0);
            assert!(next_ask_order_id == clob::order_id(3, false), 0);
            // let depth = clob::get_level2_book_status(&pool, 5 * FLOAT_SCALING, false, &clock);
            // assert!(depth == 500, 0);
            // let order = clob::get_order_status(&pool, 2 * FLOAT_SCALING,  false, clob::order_id(1, false));
            // let order_cmp = test_construct_order_with_expiration(1, 2 * FLOAT_SCALING, 500, false, account_cap_user, 0);
            // assert!(order == &order_cmp, 0);
            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order_with_expiration(
                        0,
                        5 * FLOAT_SCALING,
                        500,
                        false,
                        account_cap_user,
                        TIMESTAMP_INF
                    )
                );
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_tick_level(asks, 5 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order_with_expiration(1, 2 * FLOAT_SCALING, 500, false, account_cap_user, 0)
                );
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order_with_expiration(
                        2,
                        2 * FLOAT_SCALING,
                        1000,
                        false,
                        account_cap_user,
                        TIMESTAMP_INF
                    )
                );
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_tick_level(asks, 2 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 1 * FLOAT_SCALING, 10000, true, account_cap_user)
                );
                let (_, _, bid, _) = get_pool_stat(&pool);
                clob::check_tick_level(bid, 1 * FLOAT_SCALING, &open_orders);
            };
            test::return_shared(clock);
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };

        // test match (bid side)
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let (base_quantity_filled, quote_quantity_filled) = clob::test_match_bid(
                &mut pool,
                1500,
                MAX_PRICE,
                1,
            );
            assert!(base_quantity_filled == 1500 - 5 - 3, 0);
            // 4500
            assert!(quote_quantity_filled == 4500, 0);
            test::return_shared(pool);
        };
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);
            // rebate fee in base asset 3
            custodian::assert_user_balance<USD>(quote_custodian, account_cap_user, 4500, 10000);
            custodian::assert_user_balance<SUI>(base_custodian, account_cap_user, 8500 + 3, 0);
            {
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_empty_tick_level(asks, 2 * FLOAT_SCALING);
            };
            {
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_empty_tick_level(asks, 5 * FLOAT_SCALING);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 1 * FLOAT_SCALING, 10000, true, account_cap_user)
                );
                let (_, _, bid, _) = get_pool_stat(&pool);
                clob::check_tick_level(bid, 1 * FLOAT_SCALING, &open_orders);
            };
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        end(test)
    }

    fun test_inject_and_match_taker_ask_with_expiration_(test: Scenario): TransactionEffects {
        let (alice, bob) = people();
        // setup pool and custodian
        let owner = @0xF;
        next_tx(&mut test, owner);
        next_tx(&mut test, owner);
        {
            // taker_fee_rate = 0.005; maker_rebate_fee = 0.0025;
            clob::setup_test(5000000, 2500000, &mut test, owner);
            mint_account_cap_transfer(alice, test::ctx(&mut test));
            mint_account_cap_transfer(bob, test::ctx(&mut test));
        };
        // test inject limit order (bid side) and match
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_mut_custodian(&mut pool);
            let alice_deposit_WSUI: u64 = 10000;
            let alice_deposit_USDC: u64 = 10000;
            custodian::test_increase_user_available_balance<SUI>(base_custodian, account_cap_user, alice_deposit_WSUI);
            custodian::test_increase_user_available_balance<USD>(quote_custodian, account_cap_user, alice_deposit_USDC);
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            clob::test_inject_limit_order_with_expiration(
                &mut pool,
                5 * FLOAT_SCALING,
                500,
                true,
                TIMESTAMP_INF,
                &account_cap,
                ctx(&mut test)
            );
            clob::test_inject_limit_order_with_expiration(
                &mut pool,
                5 * FLOAT_SCALING,
                1000,
                true,
                0,
                &account_cap,
                ctx(&mut test)
            );
            clob::test_inject_limit_order_with_expiration(
                &mut pool,
                2 * FLOAT_SCALING,
                1000,
                true,
                TIMESTAMP_INF,
                &account_cap,
                ctx(&mut test)
            );
            clob::test_inject_limit_order_with_expiration(
                &mut pool,
                10 * FLOAT_SCALING,
                10000,
                false,
                TIMESTAMP_INF,
                &account_cap,
                ctx(&mut test)
            );
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);
            custodian::assert_user_balance<USD>(quote_custodian, account_cap_user, 500, 9500);
            custodian::assert_user_balance<SUI>(base_custodian, account_cap_user, 0, 10000);
            let (next_bid_order_id, next_ask_order_id, _, _) = clob::get_pool_stat(&pool);
            assert!(next_bid_order_id == clob::order_id(3, true), 0);
            assert!(next_ask_order_id == clob::order_id(1, false), 0);

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order_with_expiration(
                        0,
                        5 * FLOAT_SCALING,
                        500,
                        true,
                        account_cap_user,
                        TIMESTAMP_INF
                    )
                );
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order_with_expiration(
                        1,
                        5 * FLOAT_SCALING,
                        1000,
                        true,
                        account_cap_user,
                        0,
                    )
                );
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_tick_level(bids, 5 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order_with_expiration(
                        2,
                        2 * FLOAT_SCALING,
                        1000,
                        true,
                        account_cap_user,
                        TIMESTAMP_INF
                    )
                );
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_tick_level(bids, 2 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order_with_expiration(
                        0,
                        10 * FLOAT_SCALING,
                        10000,
                        false,
                        account_cap_user,
                        TIMESTAMP_INF
                    )
                );
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_tick_level(asks, 10 * FLOAT_SCALING, &open_orders);
            };
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };

        // test match (ask side)
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let (base_quantity_filled, quote_quantity_filled) = clob::test_match_ask(
                &mut pool,
                1500,
                MIN_PRICE,
                1,
            );
            assert!(base_quantity_filled == 1500, 0);
            assert!(quote_quantity_filled == 4500 - 13 - 10, 0);
            test::return_shared(pool);
        };
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);
            // rebate
            custodian::assert_user_balance<USD>(
                quote_custodian,
                account_cap_user,
                5500 + 6 + 5,
                9500 - 2500 - 5000 - 2000
            );
            custodian::assert_user_balance<SUI>(base_custodian, account_cap_user, 1500, 10000);
            {
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_empty_tick_level(bids, 5 * FLOAT_SCALING);
            };
            {
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_empty_tick_level(bids, 2 * FLOAT_SCALING);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 10 * FLOAT_SCALING, 10000, false, account_cap_user)
                );
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_tick_level(asks, 10 * FLOAT_SCALING, &open_orders);
            };
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        end(test)
    }

    fun test_inject_and_price_limit_affected_match_taker_bid_(test: Scenario): TransactionEffects {
        let (alice, bob) = people();
        // setup pool and custodian
        let owner = @0xFF;
        // reset pool and custodian
        next_tx(&mut test, owner);
        {
            clob::setup_test(0, 0, &mut test, owner);
            mint_account_cap_transfer(alice, test::ctx(&mut test));
            mint_account_cap_transfer(bob, test::ctx(&mut test));
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_mut_custodian(&mut pool);
            let alice_deposit_WSUI: u64 = 100;
            let alice_deposit_USDC: u64 = 10;
            custodian::test_increase_user_available_balance<SUI>(base_custodian, account_cap_user, alice_deposit_WSUI);
            custodian::test_increase_user_available_balance<USD>(quote_custodian, account_cap_user, alice_deposit_USDC);
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 2, false, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 3, false, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 2 * FLOAT_SCALING, 10, false, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 1 * FLOAT_SCALING, 10, true, &account_cap, ctx(&mut test));
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };

        // test inject limit order and match (bid side)
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);
            custodian::assert_user_balance<USD>(quote_custodian, account_cap_user, 0, 10);
            custodian::assert_user_balance<SUI>(base_custodian, account_cap_user, 85, 15);
            let (next_bid_order_id, next_ask_order_id, _, _) = clob::get_pool_stat(&pool);
            assert!(next_bid_order_id == clob::order_id(1, true), 0);
            assert!(next_ask_order_id == clob::order_id(3, false), 0);

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 5 * FLOAT_SCALING, 2, false, account_cap_user)
                );
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(1, 5 * FLOAT_SCALING, 3, false, account_cap_user)
                );
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_tick_level(asks, 5 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(2, 2 * FLOAT_SCALING, 10, false, account_cap_user)
                );
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_tick_level(asks, 2 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 1 * FLOAT_SCALING, 10, true, account_cap_user)
                );
                let (_, _, bid, _) = get_pool_stat(&pool);
                clob::check_tick_level(bid, 1 * FLOAT_SCALING, &open_orders);
            };
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };

        // test match with price limit (bid side)
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let (base_quantity_filled, quote_quantity_filled) = clob::test_match_bid(
                &mut pool,
                20,
                5 * FLOAT_SCALING,
                0
            );
            assert!(base_quantity_filled == 15, 0);
            assert!(quote_quantity_filled == 45, 0);
            test::return_shared(pool);
        };
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);
            // custodian::assert_user_balance<USDC>(&custodian, alice, 0, 10);
            // custodian::assert_user_balance<WSUI>(&custodian, alice, 85, 15);
            custodian::assert_user_balance<USD>(quote_custodian, account_cap_user, 45, 10);
            custodian::assert_user_balance<SUI>(base_custodian, account_cap_user, 85, 0);
            {
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_empty_tick_level(asks, 2 * FLOAT_SCALING);
            };
            {
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_empty_tick_level(asks, 5 * FLOAT_SCALING);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 1 * FLOAT_SCALING, 10, true, account_cap_user)
                );
                let (_, _, bid, _) = get_pool_stat(&pool);
                clob::check_tick_level(bid, 1 * FLOAT_SCALING, &open_orders);
            };
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };

        end(test)
    }

    fun test_inject_and_price_limit_affected_match_taker_ask_(test: Scenario): TransactionEffects {
        let (alice, bob) = people();
        // setup pool and custodian
        let owner = @0xFF;
        next_tx(&mut test, owner);
        {
            clob::setup_test(0, 0, &mut test, owner);
            mint_account_cap_transfer(alice, test::ctx(&mut test));
            mint_account_cap_transfer(bob, test::ctx(&mut test));
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_mut_custodian(&mut pool);
            let alice_deposit_WSUI: u64 = 10;
            let alice_deposit_USDC: u64 = 100;
            custodian::test_increase_user_available_balance<SUI>(base_custodian, account_cap_user, alice_deposit_WSUI);
            custodian::test_increase_user_available_balance<USD>(quote_custodian, account_cap_user, alice_deposit_USDC);
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        // test inject limit order and match (ask side)
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            // let account_cap_user = get_account_cap_user(&account_cap);
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 2, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 3, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 2 * FLOAT_SCALING, 10, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 20 * FLOAT_SCALING, 10, false, &account_cap, ctx(&mut test));
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_custodian(&mut pool);
            custodian::assert_user_balance<USD>(quote_custodian, account_cap_user, 55, 45);
            custodian::assert_user_balance<SUI>(base_custodian, account_cap_user, 0, 10);
            let (next_bid_order_id, next_ask_order_id, _, _) = clob::get_pool_stat(&pool);
            assert!(next_bid_order_id == clob::order_id(3, true), 0);
            assert!(next_ask_order_id == clob::order_id(1, false), 0);

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 5 * FLOAT_SCALING, 2, true, account_cap_user)
                );
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(1, 5 * FLOAT_SCALING, 3, true, account_cap_user)
                );
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_tick_level(bids, 5 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(2, 2 * FLOAT_SCALING, 10, true, account_cap_user)
                );
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_tick_level(bids, 2 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 20 * FLOAT_SCALING, 10, false, account_cap_user)
                );
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_tick_level(asks, 20 * FLOAT_SCALING, &open_orders);
            };
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };

        // test match with price limit (ask side)
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let (base_quantity_filled, quote_quantity_filled) = clob::test_match_ask(
                &mut pool,
                10,
                3 * FLOAT_SCALING,
                0,
            );
            assert!(base_quantity_filled == 5, 0);
            assert!(quote_quantity_filled == 25, 0);
            test::return_shared(pool);
        };
        next_tx(&mut test, bob);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);
            custodian::assert_user_balance<USD>(quote_custodian, account_cap_user, 55, 20);
            custodian::assert_user_balance<SUI>(base_custodian, account_cap_user, 5, 10);
            {
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_empty_tick_level(bids, 5 * FLOAT_SCALING);
            };
            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(2, 2 * FLOAT_SCALING, 10, true, account_cap_user)
                );
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_tick_level(bids, 2 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 20 * FLOAT_SCALING, 10, false, account_cap_user)
                );
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_tick_level(asks, 20 * FLOAT_SCALING, &open_orders);
            };
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        end(test)
    }

    fun test_remove_order_(test: Scenario): TransactionEffects {
        let (alice, _) = people();
        let owner: address = @0xF;
        next_tx(&mut test, owner);
        {
            clob::setup_test(0, 0, &mut test, owner);
            mint_account_cap_transfer(alice, test::ctx(&mut test));
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_mut_custodian(&mut pool);
            let alice_deposit_WSUI: u64 = 10;
            let alice_deposit_USDC: u64 = 100;
            custodian::test_increase_user_available_balance<SUI>(base_custodian, account_cap_user, alice_deposit_WSUI);
            custodian::test_increase_user_available_balance<USD>(quote_custodian, account_cap_user, alice_deposit_USDC);

            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 2, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 3, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 2 * FLOAT_SCALING, 10, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 2 * FLOAT_SCALING, 10, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 20 * FLOAT_SCALING, 10, false, &account_cap, ctx(&mut test));

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 5 * FLOAT_SCALING, 2, true, account_cap_user)
                );
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(1, 5 * FLOAT_SCALING, 3, true, account_cap_user)
                );
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_tick_level(bids, 5 * FLOAT_SCALING, &open_orders);
            };
            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(2, 2 * FLOAT_SCALING, 10, true, account_cap_user)
                );
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(3, 2 * FLOAT_SCALING, 10, true, account_cap_user)
                );
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_tick_level(bids, 2 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 20 * FLOAT_SCALING, 10, false, account_cap_user)
                );
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_tick_level(asks, 20 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 5 * FLOAT_SCALING, 2, true, account_cap_user)
                );
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(1, 5 * FLOAT_SCALING, 3, true, account_cap_user)
                );
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_tick_level(bids, 5 * FLOAT_SCALING, &open_orders);
            };

            // check usr open orders before cancel
            {
                let usr_open_orders_cmp = vector::empty<u64>();
                vector::push_back(&mut usr_open_orders_cmp, order_id(0, true));
                vector::push_back(&mut usr_open_orders_cmp, 5 * FLOAT_SCALING);
                vector::push_back(&mut usr_open_orders_cmp, order_id(1, true));
                vector::push_back(&mut usr_open_orders_cmp, 5 * FLOAT_SCALING);
                vector::push_back(&mut usr_open_orders_cmp, order_id(2, true));
                vector::push_back(&mut usr_open_orders_cmp, 2 * FLOAT_SCALING);
                vector::push_back(&mut usr_open_orders_cmp, order_id(3, true));
                vector::push_back(&mut usr_open_orders_cmp, 2 * FLOAT_SCALING);
                vector::push_back(&mut usr_open_orders_cmp, order_id(0, false));
                vector::push_back(&mut usr_open_orders_cmp, 20 * FLOAT_SCALING);
                clob::check_usr_open_orders(clob::get_usr_open_orders(&pool, account_cap_user), &usr_open_orders_cmp);
                let user_open_orders = clob::get_usr_open_orders(&mut pool, account_cap_user);
                clob::check_usr_open_orders(user_open_orders, &usr_open_orders_cmp);
            };

            clob::test_remove_order(&mut pool, 0, 0, true, account_cap_user);
            {
                // check tick level from pool after remove
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(1, 5 * FLOAT_SCALING, 3, true, account_cap_user)
                );
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_tick_level(bids, 5 * FLOAT_SCALING, &open_orders);
                // check usr open orders after remove order bid-0
                let usr_open_orders_cmp = vector::empty<u64>();
                vector::push_back(&mut usr_open_orders_cmp, order_id(0, false));
                vector::push_back(&mut usr_open_orders_cmp, 20 * FLOAT_SCALING);
                vector::push_back(&mut usr_open_orders_cmp, order_id(1, true));
                vector::push_back(&mut usr_open_orders_cmp, 5 * FLOAT_SCALING);
                vector::push_back(&mut usr_open_orders_cmp, order_id(2, true));
                vector::push_back(&mut usr_open_orders_cmp, 2 * FLOAT_SCALING);
                vector::push_back(&mut usr_open_orders_cmp, order_id(3, true));
                vector::push_back(&mut usr_open_orders_cmp, 2 * FLOAT_SCALING);
                let user_open_orders = clob::get_usr_open_orders(&mut pool, account_cap_user);
                clob::check_usr_open_orders(user_open_orders, &usr_open_orders_cmp);
            };

            clob::test_remove_order(&mut pool, 0, 1, true, account_cap_user);
            {
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_empty_tick_level(bids, 5 * FLOAT_SCALING);
                let usr_open_orders_cmp = vector::empty<u64>();
                vector::push_back(&mut usr_open_orders_cmp, order_id(0, false));
                vector::push_back(&mut usr_open_orders_cmp, 20 * FLOAT_SCALING);
                vector::push_back(&mut usr_open_orders_cmp, order_id(3, true));
                vector::push_back(&mut usr_open_orders_cmp, 2 * FLOAT_SCALING);
                vector::push_back(&mut usr_open_orders_cmp, order_id(2, true));
                vector::push_back(&mut usr_open_orders_cmp, 2 * FLOAT_SCALING);
                clob::check_usr_open_orders(
                    clob::get_usr_open_orders(&mut pool, account_cap_user),
                    &usr_open_orders_cmp
                );
            };

            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        end(test)
    }


    fun test_remove_all_orders_(test: Scenario): TransactionEffects {
        let (alice, _) = people();
        let owner: address = @0xF;
        next_tx(&mut test, owner);
        {
            clob::setup_test(0, 0, &mut test, owner);
            mint_account_cap_transfer(alice, test::ctx(&mut test));
        };
        next_tx(&mut test, alice);
        {
            let pool = test::take_shared<Pool<SUI, USD>>(&mut test);
            let account_cap = test::take_from_address<AccountCap>(&test, alice);
            let account_cap_user = object::id(&account_cap);
            let (base_custodian, quote_custodian) = clob::borrow_mut_custodian(&mut pool);
            let alice_deposit_WSUI: u64 = 10;
            let alice_deposit_USDC: u64 = 100;
            custodian::test_increase_user_available_balance<SUI>(base_custodian, account_cap_user, alice_deposit_WSUI);
            custodian::test_increase_user_available_balance<USD>(quote_custodian, account_cap_user, alice_deposit_USDC);

            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 2, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 5 * FLOAT_SCALING, 3, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 2 * FLOAT_SCALING, 10, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 2 * FLOAT_SCALING, 10, true, &account_cap, ctx(&mut test));
            clob::test_inject_limit_order(&mut pool, 20 * FLOAT_SCALING, 10, false, &account_cap, ctx(&mut test));

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 5 * FLOAT_SCALING, 2, true, account_cap_user)
                );
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(1, 5 * FLOAT_SCALING, 3, true, account_cap_user)
                );
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_tick_level(bids, 5 * FLOAT_SCALING, &open_orders);
            };
            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(2, 2 * FLOAT_SCALING, 10, true, account_cap_user)
                );
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(3, 2 * FLOAT_SCALING, 10, true, account_cap_user)
                );
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_tick_level(bids, 2 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 20 * FLOAT_SCALING, 10, false, account_cap_user)
                );
                let (_, _, _, asks) = get_pool_stat(&pool);
                clob::check_tick_level(asks, 20 * FLOAT_SCALING, &open_orders);
            };

            {
                let open_orders = vector::empty<Order>();
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(0, 5 * FLOAT_SCALING, 2, true, account_cap_user)
                );
                vector::push_back(
                    &mut open_orders,
                    clob::test_construct_order(1, 5 * FLOAT_SCALING, 3, true, account_cap_user)
                );
                let (_, _, bids, _) = get_pool_stat(&pool);
                clob::check_tick_level(bids, 5 * FLOAT_SCALING, &open_orders);
            };

            // check usr open orders before cancel
            {
                let usr_open_orders_cmp = vector::empty<u64>();
                vector::push_back(&mut usr_open_orders_cmp, order_id(0, true));
                vector::push_back(&mut usr_open_orders_cmp, 5 * FLOAT_SCALING);
                vector::push_back(&mut usr_open_orders_cmp, order_id(1, true));
                vector::push_back(&mut usr_open_orders_cmp, 5 * FLOAT_SCALING);
                vector::push_back(&mut usr_open_orders_cmp, order_id(2, true));
                vector::push_back(&mut usr_open_orders_cmp, 2 * FLOAT_SCALING);
                vector::push_back(&mut usr_open_orders_cmp, order_id(3, true));
                vector::push_back(&mut usr_open_orders_cmp, 2 * FLOAT_SCALING);
                vector::push_back(&mut usr_open_orders_cmp, order_id(0, false));
                vector::push_back(&mut usr_open_orders_cmp, 20 * FLOAT_SCALING);
                clob::check_usr_open_orders(clob::get_usr_open_orders(&pool, account_cap_user), &usr_open_orders_cmp);
                let user_open_orders = clob::get_usr_open_orders(&mut pool, account_cap_user);
                clob::check_usr_open_orders(user_open_orders, &usr_open_orders_cmp);
            };

            clob::cancel_all_orders(&mut pool, &account_cap);
            {
                let usr_open_orders_cmp = vector::empty<u64>();
                let user_open_orders = clob::get_usr_open_orders(&mut pool, account_cap_user);
                clob::check_usr_open_orders(user_open_orders, &usr_open_orders_cmp);

                // check tick level from pool after remove
                let (_, _, bids, asks) = get_pool_stat(&pool);
                clob::check_empty_tick_level(bids, 5 * FLOAT_SCALING);
                clob::check_empty_tick_level(bids, 2 * FLOAT_SCALING);
                clob::check_empty_tick_level(asks, 20 * FLOAT_SCALING);
                let (base_custodian, quote_custodian) = clob::borrow_custodian(&pool);
                custodian::assert_user_balance(base_custodian, account_cap_user, 10, 0);
                custodian::assert_user_balance(quote_custodian, account_cap_user, 100, 0);
                let (base_avail, base_locked, quote_avail, quote_locked) = account_balance(&pool, &account_cap);
                assert!(base_avail == 10, 0);
                assert!(base_locked == 0, 0);
                assert!(quote_avail == 100, 0);
                assert!(quote_locked == 0, 0);
            };
            test::return_shared(pool);
            test::return_to_address<AccountCap>(alice, account_cap);
        };
        end(test)
    }


    fun scenario(): Scenario { test::begin(@0x1) }

    fun people(): (address, address) { (@0xBEEF, @0x1337) }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module deepbook::clob {
    use std::type_name::TypeName;

    use sui::balance::Balance;
    use sui::clock::{Self, Clock};
    use sui::coin::Coin;
    use sui::event;
    use sui::linked_table::{Self, LinkedTable};
    use sui::sui::SUI;
    use sui::table::{Self, Table, contains, borrow_mut};

    use deepbook::critbit::{Self, CritbitTree, borrow_mut_leaf_by_index, remove_leaf_by_index, borrow_leaf_by_index, borrow_leaf_by_key, find_leaf};
    use deepbook::custodian::{Self, Custodian, AccountCap};
    use deepbook::math::Self as clob_math;

    // <<<<<<<<<<<<<<<<<<<<<<<< Error codes <<<<<<<<<<<<<<<<<<<<<<<<
    const DEPRECATED: u64 = 0;
    const EInvalidOrderId: u64 = 3;
    const EUnauthorizedCancel: u64 = 4;
    const EInvalidQuantity: u64 = 6;
    const EInvalidTickPrice: u64 = 11;
    const EInvalidUser: u64 = 12;

    // <<<<<<<<<<<<<<<<<<<<<<<< Error codes <<<<<<<<<<<<<<<<<<<<<<<<

    // <<<<<<<<<<<<<<<<<<<<<<<< Constants <<<<<<<<<<<<<<<<<<<<<<<<
    const MIN_ASK_ORDER_ID: u64 = 1 << 63;

    // <<<<<<<<<<<<<<<<<<<<<<<< Constants <<<<<<<<<<<<<<<<<<<<<<<<

    // <<<<<<<<<<<<<<<<<<<<<<<< Events <<<<<<<<<<<<<<<<<<<<<<<<

    #[allow(unused_field)]
    /// Emitted when a new pool is created
    public struct PoolCreated has copy, store, drop {
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

    #[allow(unused_field)]
    /// Emitted when a maker order is injected into the order book.
    public struct OrderPlacedV2<phantom BaseAsset, phantom QuoteAsset> has copy, store, drop {
        /// object ID of the pool the order was placed on
        pool_id: ID,
        /// ID of the order within the pool
        order_id: u64,
        is_bid: bool,
        /// object ID of the `AccountCap` that placed the order
        owner: ID,
        base_asset_quantity_placed: u64,
        price: u64,
        expire_timestamp: u64
    }

    /// Emitted when a maker order is canceled.
    public struct OrderCanceled<phantom BaseAsset, phantom QuoteAsset> has copy, store, drop {
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

    #[allow(unused_field)]
    /// Emitted only when a maker order is filled.
    public struct OrderFilledV2<phantom BaseAsset, phantom QuoteAsset> has copy, store, drop {
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
        price: u64,
        taker_commission: u64,
        maker_rebates: u64
    }
    // <<<<<<<<<<<<<<<<<<<<<<<< Events <<<<<<<<<<<<<<<<<<<<<<<<

    public struct Order has store, drop {
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

    public struct TickLevel has store {
        price: u64,
        // The key is order order id.
        open_orders: LinkedTable<u64, Order>,
        // other price level info
    }

    #[allow(unused_field)]
    public struct Pool<phantom BaseAsset, phantom QuoteAsset> has key {
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
        // Stores the fee paid to create this pool. These funds are not accessible.
        creation_fee: Balance<SUI>,
        // Deprecated.
        base_asset_trading_fees: Balance<BaseAsset>,
        // Stores the trading fees paid in `QuoteAsset`. These funds are not accessible.
        quote_asset_trading_fees: Balance<QuoteAsset>,
    }

    fun destroy_empty_level(level: TickLevel) {
        let TickLevel {
            price: _,
            open_orders: orders,
        } = level;

        linked_table::destroy_empty(orders);
    }

    #[deprecated(note = b"Creating account is deprecated. Please use Deepbook V3.")]
    public fun create_account(_ctx: &mut TxContext): AccountCap {
        abort DEPRECATED
    }

    #[deprecated, allow(unused_type_parameter)]
    public fun create_pool<BaseAsset, QuoteAsset>(
        _tick_size: u64,
        _lot_size: u64,
        _creation_fee: Coin<SUI>,
        _ctx: &mut TxContext,
    ) {
        abort DEPRECATED
    }

    #[deprecated(note = b"Depositing is deprecated. Please use Deepbook V3.")]
    public fun deposit_base<BaseAsset, QuoteAsset>(
        _pool: &mut Pool<BaseAsset, QuoteAsset>,
        _coin: Coin<BaseAsset>,
        _account_cap: &AccountCap
    ) {

        abort 1337
    }

    #[deprecated(note = b"Depositing is deprecated. Please use Deepbook V3.")]
    public fun deposit_quote<BaseAsset, QuoteAsset>(
        _pool: &mut Pool<BaseAsset, QuoteAsset>,
        _coin: Coin<QuoteAsset>,
        _account_cap: &AccountCap
    ) {

        abort 1337
    }

    public fun withdraw_base<BaseAsset, QuoteAsset>(
        pool: &mut Pool<BaseAsset, QuoteAsset>,
        quantity: u64,
        account_cap: &AccountCap,
        ctx: &mut TxContext
    ): Coin<BaseAsset> {
        assert!(quantity > 0, EInvalidQuantity);
        custodian::withdraw_asset(&mut pool.base_custodian, quantity, account_cap, ctx)
    }

    public fun withdraw_quote<BaseAsset, QuoteAsset>(
        pool: &mut Pool<BaseAsset, QuoteAsset>,
        quantity: u64,
        account_cap: &AccountCap,
        ctx: &mut TxContext
    ): Coin<QuoteAsset> {
        assert!(quantity > 0, EInvalidQuantity);
        custodian::withdraw_asset(&mut pool.quote_custodian, quantity, account_cap, ctx)
    }

    #[deprecated(note = b"Swapping is deprecated. Please use Deepbook V3.")]
    // for smart routing
    public fun swap_exact_base_for_quote<BaseAsset, QuoteAsset>(
        _pool: &mut Pool<BaseAsset, QuoteAsset>,
        _quantity: u64,
        _base_coin: Coin<BaseAsset>,
        _quote_coin: Coin<QuoteAsset>,
        _clock: &Clock,
        _ctx: &mut TxContext,
    ): (Coin<BaseAsset>, Coin<QuoteAsset>, u64) {

        abort 1337
    }

    #[deprecated(note = b"Swapping is deprecated. Please use Deepbook V3.")]
    // for smart routing
    public fun swap_exact_quote_for_base<BaseAsset, QuoteAsset>(
        _pool: &mut Pool<BaseAsset, QuoteAsset>,
        _quantity: u64,
        _clock: &Clock,
        _quote_coin: Coin<QuoteAsset>,
        _ctx: &mut TxContext,
    ): (Coin<BaseAsset>, Coin<QuoteAsset>, u64) {

        abort 1337
    }

    #[deprecated(note = b"Placing market order is deprecated. Please use Deepbook V3.")]
    /// Place a market order to the order book.
    public fun place_market_order<BaseAsset, QuoteAsset>(
        _pool: &mut Pool<BaseAsset, QuoteAsset>,
        _quantity: u64,
        _is_bid: bool,
        mut _base_coin: Coin<BaseAsset>,
        mut _quote_coin: Coin<QuoteAsset>,
        _clock: &Clock,
        _ctx: &mut TxContext,
    ): (Coin<BaseAsset>, Coin<QuoteAsset>) {

        abort 1337
    }

    #[deprecated(note = b"Placing limit order is deprecated. Please use Deepbook V3.")]
    /// Place a limit order to the order book.
    /// Returns (base quantity filled, quote quantity filled, whether a maker order is being placed, order id of the maker order).
    /// When the limit order is not successfully placed, we return false to indicate that and also returns a meaningless order_id 0.
    /// When the limit order is successfully placed, we return true to indicate that and also the corresponding order_id.
    /// So please check that boolean value first before using the order id.
    public fun place_limit_order<BaseAsset, QuoteAsset>(
        _pool: &mut Pool<BaseAsset, QuoteAsset>,
        _price: u64,
        _quantity: u64,
        _is_bid: bool,
        _expire_timestamp: u64, // Expiration timestamp in ms in absolute value inclusive.
        _restriction: u8,
        _clock: &Clock,
        _account_cap: &AccountCap,
        _ctx: &mut TxContext
    ): (u64, u64, bool, u64) {

       abort 1337
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
        let order = remove_order(
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

    fun remove_order(
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
        assert!(contains(&pool.usr_open_orders, user), EInvalidUser);
        let usr_open_order_ids = table::borrow_mut(&mut pool.usr_open_orders, user);
        while (!linked_table::is_empty(usr_open_order_ids)) {
            let order_id = *option::borrow(linked_table::back(usr_open_order_ids));
            let order_price = *linked_table::borrow(usr_open_order_ids, order_id);
            let is_bid = order_is_bid(order_id);
            let open_orders =
                if (is_bid) { &mut pool.bids }
                else { &mut pool.asks };
            let (_, tick_index) = critbit::find_leaf(open_orders, order_price);
            let order = remove_order(
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
        let mut tick_index: u64 = 0;
        let mut tick_price: u64 = 0;
        let n_order = vector::length(&order_ids);
        let mut i_order = 0;
        let usr_open_orders = borrow_mut(&mut pool.usr_open_orders, user);
        while (i_order < n_order) {
            let order_id = *vector::borrow(&order_ids, i_order);
            assert!(linked_table::contains(usr_open_orders, order_id), EInvalidOrderId);
            let new_tick_price = *linked_table::borrow(usr_open_orders, order_id);
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
            let order = remove_order(
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
        let mut open_orders = vector::empty<Order>();
        let mut order_id = linked_table::front(usr_open_order_ids);
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

    /// Query the market price of order book
    /// returns (best_bid_price, best_ask_price)
    public fun get_market_price<BaseAsset, QuoteAsset>(
        pool: &Pool<BaseAsset, QuoteAsset>
    ): (u64, u64){
        let (bid_price, _) = critbit::max_leaf(&pool.bids);
        let (ask_price, _) = critbit::min_leaf(&pool.asks);
        return (bid_price, ask_price)
    }

    /// Enter a price range and return the level2 order depth of all valid prices within this price range in bid side
    /// returns two vectors of u64
    /// The previous is a list of all valid prices
    /// The latter is the corresponding depth list
    public fun get_level2_book_status_bid_side<BaseAsset, QuoteAsset>(
        pool: &Pool<BaseAsset, QuoteAsset>,
        mut price_low: u64,
        mut price_high: u64,
        clock: &Clock
    ): (vector<u64>, vector<u64>) {
        let (price_low_, _) = critbit::min_leaf(&pool.bids);
        if (price_low < price_low_) price_low = price_low_;
        let (price_high_, _) = critbit::max_leaf(&pool.bids);
        if (price_high > price_high_) price_high = price_high_;
        price_low = critbit::find_closest_key(&pool.bids, price_low);
        price_high = critbit::find_closest_key(&pool.bids, price_high);
        let mut price_vec = vector::empty<u64>();
        let mut depth_vec = vector::empty<u64>();
        if (price_low == 0) { return (price_vec, depth_vec) };
        while (price_low <= price_high) {
            let depth = get_level2_book_status(
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
        mut price_low: u64,
        mut price_high: u64,
        clock: &Clock
    ): (vector<u64>, vector<u64>) {
        let (price_low_, _) = critbit::min_leaf(&pool.asks);
        if (price_low < price_low_) price_low = price_low_;
        let (price_high_, _) = critbit::max_leaf(&pool.asks);
        if (price_high > price_high_) price_high = price_high_;
        price_low = critbit::find_closest_key(&pool.asks, price_low);
        price_high = critbit::find_closest_key(&pool.asks, price_high);
        let mut price_vec = vector::empty<u64>();
        let mut depth_vec = vector::empty<u64>();
        if (price_low == 0) { return (price_vec, depth_vec) };
        while (price_low <= price_high) {
            let depth = get_level2_book_status(
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

    /// internal func to retrieve single depth of a tick price
    fun get_level2_book_status(
        open_orders: &CritbitTree<TickLevel>,
        price: u64,
        time_stamp: u64
    ): u64 {
        let tick_level = critbit::borrow_leaf_by_key(open_orders, price);
        let tick_open_orders = &tick_level.open_orders;
        let mut depth = 0;
        let mut order_id = linked_table::front(tick_open_orders);
        let mut order: &Order;
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
        assert!(table::contains(&pool.usr_open_orders, user), EInvalidUser);
        let usr_open_order_ids = table::borrow(&pool.usr_open_orders, user);
        assert!(linked_table::contains(usr_open_order_ids, order_id), EInvalidOrderId);
        let order_price = *linked_table::borrow(usr_open_order_ids, order_id);
        let open_orders =
            if (order_id < MIN_ASK_ORDER_ID) { &pool.bids }
            else { &pool.asks };
        let tick_level = critbit::borrow_leaf_by_key(open_orders, order_price);
        let tick_open_orders = &tick_level.open_orders;
        let order = linked_table::borrow(tick_open_orders, order_id);
        order
    }

    // === Deprecated ===
    #[allow(unused_field)]
    /// Deprecated since v1.0.0, use `OrderPlacedV2` instead.
    public struct OrderPlaced<phantom BaseAsset, phantom QuoteAsset> has copy, store, drop {
        /// object ID of the pool the order was placed on
        pool_id: ID,
        /// ID of the order within the pool
        order_id: u64,
        is_bid: bool,
        /// object ID of the `AccountCap` that placed the order
        owner: ID,
        base_asset_quantity_placed: u64,
        price: u64,
    }

    #[allow(unused_field)]
    /// Deprecated since v1.0.0, use `OrderFilledV2` instead.
    public struct OrderFilled<phantom BaseAsset, phantom QuoteAsset> has copy, store, drop {
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

}

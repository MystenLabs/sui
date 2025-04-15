// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module deepbook::clob_v2 {
    use std::type_name::TypeName;

    use sui::balance::{Self, Balance};
    use sui::clock::{Self, Clock};
    use sui::coin::{Self, Coin};
    use sui::event;
    use sui::linked_table::{Self, LinkedTable};
    use sui::sui::SUI;
    use sui::table::{Self, Table, contains, borrow_mut};

    use deepbook::critbit::{Self, CritbitTree, borrow_mut_leaf_by_index, remove_leaf_by_index, borrow_leaf_by_index, borrow_leaf_by_key, find_leaf};
    use deepbook::custodian_v2::{Self as custodian, Custodian, AccountCap, account_owner};
    use deepbook::math::Self as clob_math;

    // <<<<<<<<<<<<<<<<<<<<<<<< Error codes <<<<<<<<<<<<<<<<<<<<<<<<
    const EIncorrectPoolOwner: u64 = 1;
    const EInvalidOrderId: u64 = 3;
    const EUnauthorizedCancel: u64 = 4;
    const EInvalidQuantity: u64 = 6;
    const EInvalidTickPrice: u64 = 11;
    const EInvalidUser: u64 = 12;
    const ENotEqual: u64 = 13;
    const EInvalidExpireTimestamp: u64 = 19;

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
    public struct OrderPlaced<phantom BaseAsset, phantom QuoteAsset> has copy, store, drop {
        /// object ID of the pool the order was placed on
        pool_id: ID,
        /// ID of the order within the pool
        order_id: u64,
        /// ID of the order defined by client
        client_order_id: u64,
        is_bid: bool,
        /// owner ID of the `AccountCap` that placed the order
        owner: address,
        original_quantity: u64,
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
        /// ID of the order defined by client
        client_order_id: u64,
        is_bid: bool,
        /// owner ID of the `AccountCap` that canceled the order
        owner: address,
        original_quantity: u64,
        base_asset_quantity_canceled: u64,
        price: u64
    }

    /// A struct to make all orders canceled a more efficient struct
    public struct AllOrdersCanceledComponent<phantom BaseAsset, phantom QuoteAsset> has copy, store, drop {
        /// ID of the order within the pool
        order_id: u64,
        /// ID of the order defined by client
        client_order_id: u64,
        is_bid: bool,
        /// owner ID of the `AccountCap` that canceled the order
        owner: address,
        original_quantity: u64,
        base_asset_quantity_canceled: u64,
        price: u64
    }

    /// Emitted when batch of orders are canceled.
    public struct AllOrdersCanceled<phantom BaseAsset, phantom QuoteAsset> has copy, store, drop {
        /// object ID of the pool the order was placed on
        pool_id: ID,
        orders_canceled: vector<AllOrdersCanceledComponent<BaseAsset, QuoteAsset>>,
    }

    #[allow(unused_field)]
    /// Emitted only when a maker order is filled.
    public struct OrderFilled<phantom BaseAsset, phantom QuoteAsset> has copy, store, drop {
        /// object ID of the pool the order was placed on
        pool_id: ID,
        /// ID of the order within the pool
        order_id: u64,
        /// ID of the order defined by taker client
        taker_client_order_id: u64,
        /// ID of the order defined by maker client
        maker_client_order_id: u64,
        is_bid: bool,
        /// owner ID of the `AccountCap` that filled the order
        taker_address: address,
        /// owner ID of the `AccountCap` that placed the order
        maker_address: address,
        original_quantity: u64,
        base_asset_quantity_filled: u64,
        base_asset_quantity_remaining: u64,
        price: u64,
        taker_commission: u64,
        maker_rebates: u64
    }

    #[allow(unused_field)]
    /// Emitted when user deposit asset to custodian
    public struct DepositAsset<phantom Asset> has copy, store, drop {
        /// object id of the pool that asset deposit to
        pool_id: ID,
        /// quantity of the asset deposited
        quantity: u64,
        /// owner address of the `AccountCap` that deposit the asset
        owner: address
    }

    /// Emitted when user withdraw asset from custodian
    public struct WithdrawAsset<phantom Asset> has copy, store, drop {
        /// object id of the pool that asset withdraw from
        pool_id: ID,
        /// quantity of the asset user withdrew
        quantity: u64,
        /// owner ID of the `AccountCap` that withdrew the asset
        owner: address
    }

    #[allow(unused_field)]
    /// Returned as metadata only when a maker order is filled from place order functions.
    public struct MatchedOrderMetadata<phantom BaseAsset, phantom QuoteAsset> has copy, store, drop {
        /// object ID of the pool the order was placed on
        pool_id: ID,
        /// ID of the order within the pool
        order_id: u64,
        /// Direction of order.
        is_bid: bool,
        /// owner ID of the `AccountCap` that filled the order
        taker_address: address,
        /// owner ID of the `AccountCap` that placed the order
        maker_address: address,
        /// qty of base asset filled.
        base_asset_quantity_filled: u64,
        /// price at which basset asset filled.
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
        // The highest bit of the order id is used to denote the order type, 0 for bid, 1 for ask.
        order_id: u64,
        client_order_id: u64,
        // Only used for limit orders.
        price: u64,
        // quantity when the order first placed in
        original_quantity: u64,
        // quantity of the order currently held
        quantity: u64,
        is_bid: bool,
        /// Order can only be canceled by the `AccountCap` with this owner ID
        owner: address,
        // Expiration timestamp in ms.
        expire_timestamp: u64,
        // reserved field for prevent self_matching
        self_matching_prevention: u8
    }

    public struct TickLevel has store {
        price: u64,
        // The key is order's order_id.
        open_orders: LinkedTable<u64, Order>,
    }

    #[allow(unused_field)]
    public struct Pool<phantom BaseAsset, phantom QuoteAsset> has key, store {
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
        // Map from AccountCap owner ID -> (map from order id -> order price)
        usr_open_orders: Table<address, LinkedTable<u64, u64>>,
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
        // Stores the trading fees paid in `QuoteAsset`. These funds are not accessible in the V1 of the Pools, but V2 Pools are accessible.
        quote_asset_trading_fees: Balance<QuoteAsset>,
    }

    /// Capability granting permission to access an entry in `Pool.quote_asset_trading_fees`.
    /// The pool objects created for older pools do not have a PoolOwnerCap because they were created
    /// prior to the addition of this feature. Here is a list of 11 pools on mainnet that
    /// do not have this capability:
    /// 0x31d1790e617eef7f516555124155b28d663e5c600317c769a75ee6336a54c07f
    /// 0x6e417ee1c12ad5f2600a66bc80c7bd52ff3cb7c072d508700d17cf1325324527
    /// 0x17625f1a241d34d2da0dc113086f67a2b832e3e8cd8006887c195cd24d3598a3
    /// 0x276ff4d99ecb3175091ba4baffa9b07590f84e2344e3f16e95d30d2c1678b84c
    /// 0xd1f0a9baacc1864ab19534e2d4c5d6c14f2e071a1f075e8e7f9d51f2c17dc238
    /// 0x4405b50d791fd3346754e8171aaab6bc2ed26c2c46efdd033c14b30ae507ac33
    /// 0xf0f663cf87f1eb124da2fc9be813e0ce262146f3df60bc2052d738eb41a25899
    /// 0xd9e45ab5440d61cc52e3b2bd915cdd643146f7593d587c715bc7bfa48311d826
    /// 0x5deafda22b6b86127ea4299503362638bea0ca33bb212ea3a67b029356b8b955
    /// 0x7f526b1263c4b91b43c9e646419b5696f424de28dda3c1e6658cc0a54558baa7
    /// 0x18d871e3c3da99046dfc0d3de612c5d88859bc03b8f0568bd127d0e70dbc58be
    public struct PoolOwnerCap has key, store {
        id: UID,
        /// The owner of this AccountCap. Note: this is
        /// derived from an object ID, not a user address
        owner: address
    }

    /// Accessor functions
    public fun usr_open_orders_exist<BaseAsset, QuoteAsset>(
        pool: &Pool<BaseAsset, QuoteAsset>,
        owner: address
    ): bool {
        table::contains(&pool.usr_open_orders, owner)
    }

    public fun usr_open_orders_for_address<BaseAsset, QuoteAsset>(
        pool: &Pool<BaseAsset, QuoteAsset>,
        owner: address
    ): &LinkedTable<u64, u64> {
        table::borrow(&pool.usr_open_orders, owner)
    }

    public fun usr_open_orders<BaseAsset, QuoteAsset>(
        pool: &Pool<BaseAsset, QuoteAsset>,
    ): &Table<address, LinkedTable<u64, u64>> {
        &pool.usr_open_orders
    }

    /// Function to withdraw fees created from a pool
    public fun withdraw_fees<BaseAsset, QuoteAsset>(
        pool_owner_cap: &PoolOwnerCap,
        pool: &mut Pool<BaseAsset, QuoteAsset>,
        ctx: &mut TxContext,
    ): Coin<QuoteAsset> {
        assert!(pool_owner_cap.owner == object::uid_to_address(&pool.id), EIncorrectPoolOwner);
        let quantity = quote_asset_trading_fees_value(pool);
        let to_withdraw = balance::split(&mut pool.quote_asset_trading_fees, quantity);
        coin::from_balance(to_withdraw, ctx)
    }

    /// Destroy the given `pool_owner_cap` object
    public fun delete_pool_owner_cap(pool_owner_cap: PoolOwnerCap) {
        let PoolOwnerCap { id, owner: _ } = pool_owner_cap;
        object::delete(id)
    }

    fun destroy_empty_level(level: TickLevel) {
        let TickLevel {
            price: _,
            open_orders: orders,
        } = level;

        linked_table::destroy_empty(orders);
    }

    #[deprecated(note = b"Creating new account is deprecated in Deepbook V2. Please use Deepbook V3.")]
    public fun create_account(_ctx: &mut TxContext): AccountCap {
        
        abort 1337
    }

    #[deprecated(note = b"Creating new pool is deprecated in Deepbook V2. Please use Deepbook V3."), allow(unused_type_parameter)]
    public fun create_pool<BaseAsset, QuoteAsset>(
        _tick_size: u64,
        _lot_size: u64,
        _creation_fee: Coin<SUI>,
        _ctx: &mut TxContext,
    ) {
        
        abort 1337
    }

    #[deprecated(note = b"Creating new pool is deprecated in Deepbook V2. Please use Deepbook V3."), allow(unused_type_parameter)]
    /// Function for creating pool with customized taker fee rate and maker rebate rate.
    /// The taker_fee_rate should be greater than or equal to the maker_rebate_rate, and both should have a scaling of 10^9.
    /// Taker_fee_rate of 0.25% should be 2_500_000 for example
    public fun create_customized_pool<BaseAsset, QuoteAsset>(
        _tick_size: u64,
        _lot_size: u64,
        _taker_fee_rate: u64,
        _maker_rebate_rate: u64,
        _creation_fee: Coin<SUI>,
        _ctx: &mut TxContext,
    ) {
        
        abort 1337
    }

    #[deprecated(note = b"Creating new pool is deprecated in Deepbook V2. Please use Deepbook V3.")]
    /// Function for creating an external pool. This API can be used to wrap deepbook pools into other objects.
    public fun create_pool_with_return<BaseAsset, QuoteAsset>(
        _tick_size: u64,
        _lot_size: u64,
        _creation_fee: Coin<SUI>,
        _ctx: &mut TxContext,
    ): Pool<BaseAsset, QuoteAsset> {
        
        abort 1337
    }

    #[deprecated(note = b"Creating new pool is deprecated in Deepbook V2. Please use Deepbook V3."), allow(lint(self_transfer))]
    /// Function for creating pool with customized taker fee rate and maker rebate rate.
    /// The taker_fee_rate should be greater than or equal to the maker_rebate_rate, and both should have a scaling of 10^9.
    /// Taker_fee_rate of 0.25% should be 2_500_000 for example
    public fun create_customized_pool_with_return<BaseAsset, QuoteAsset>(
        _tick_size: u64,
        _lot_size: u64,
        _taker_fee_rate: u64,
        _maker_rebate_rate: u64,
        _creation_fee: Coin<SUI>,
        _ctx: &mut TxContext,
    ) : Pool<BaseAsset, QuoteAsset> {
        
        abort 1337
    }

    #[deprecated(note = b"Creating new pool is deprecated in Deepbook V2. Please use Deepbook V3.")]
    /// A V2 function for creating customized pools for better PTB friendliness/compostability.
    /// If a user wants to create a pool and then destroy/lock the pool_owner_cap one can do
    /// so with this function.
    public fun create_customized_pool_v2<BaseAsset, QuoteAsset>(
        _tick_size: u64,
        _lot_size: u64,
        _taker_fee_rate: u64,
        _maker_rebate_rate: u64,
        _creation_fee: Coin<SUI>,
        _ctx: &mut TxContext,
    ) : (Pool<BaseAsset, QuoteAsset>, PoolOwnerCap) {
        
        abort 1337
    }

    #[deprecated(note = b"Depositing is deprecated in Deepbook V2. Please use Deepbook V3.")]
    public fun deposit_base<BaseAsset, QuoteAsset>(
        _pool: &mut Pool<BaseAsset, QuoteAsset>,
        _coin: Coin<BaseAsset>,
        _account_cap: &AccountCap
    ) {
        
        abort 1337
    }

    #[deprecated(note = b"Depositing is deprecated in Deepbook V2. Please use Deepbook V3.")]
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
        event::emit(WithdrawAsset<BaseAsset>{
            pool_id: *object::uid_as_inner(&pool.id),
            quantity,
            owner: account_owner(account_cap)
        });
        custodian::withdraw_asset(&mut pool.base_custodian, quantity, account_cap, ctx)
    }

    public fun withdraw_quote<BaseAsset, QuoteAsset>(
        pool: &mut Pool<BaseAsset, QuoteAsset>,
        quantity: u64,
        account_cap: &AccountCap,
        ctx: &mut TxContext
    ): Coin<QuoteAsset> {
        assert!(quantity > 0, EInvalidQuantity);
        event::emit(WithdrawAsset<QuoteAsset>{
            pool_id: *object::uid_as_inner(&pool.id),
            quantity,
            owner: account_owner(account_cap)
        });
        custodian::withdraw_asset(&mut pool.quote_custodian, quantity, account_cap, ctx)
    }

    #[deprecated(note = b"Swapping is deprecated in Deepbook V2. Please use Deepbook V3.")]
    // for smart routing
    public fun swap_exact_base_for_quote<BaseAsset, QuoteAsset>(
        _pool: &mut Pool<BaseAsset, QuoteAsset>,
        _client_order_id: u64,
        _account_cap: &AccountCap,
        _quantity: u64,
        _base_coin: Coin<BaseAsset>,
        _quote_coin: Coin<QuoteAsset>,
        _clock: &Clock,
        _ctx: &mut TxContext,
    ): (Coin<BaseAsset>, Coin<QuoteAsset>, u64) {
        
        abort 1337
    }

    #[deprecated(note = b"Swapping is deprecated in Deepbook V2. Please use Deepbook V3.")]
    // for smart routing
    public fun swap_exact_base_for_quote_with_metadata<BaseAsset, QuoteAsset>(
        _pool: &mut Pool<BaseAsset, QuoteAsset>,
        _client_order_id: u64,
        _account_cap: &AccountCap,
        _quantity: u64,
        _base_coin: Coin<BaseAsset>,
        _quote_coin: Coin<QuoteAsset>,
        _clock: &Clock,
        _ctx: &mut TxContext,
    ): (Coin<BaseAsset>, Coin<QuoteAsset>, u64, vector<MatchedOrderMetadata<BaseAsset, QuoteAsset>>) {
        
        abort 1337
    }

    #[deprecated(note = b"Swapping is deprecated in Deepbook V2. Please use Deepbook V3.")]
    // for smart routing
    public fun swap_exact_quote_for_base<BaseAsset, QuoteAsset>(
        _pool: &mut Pool<BaseAsset, QuoteAsset>,
        _client_order_id: u64,
        _account_cap: &AccountCap,
        _quantity: u64,
        _clock: &Clock,
        _quote_coin: Coin<QuoteAsset>,
        _ctx: &mut TxContext,
    ): (Coin<BaseAsset>, Coin<QuoteAsset>, u64) {
       
       abort 1337
    }

    #[deprecated(note = b"Swapping is deprecated in Deepbook V2. Please use Deepbook V3.")]
    public fun swap_exact_quote_for_base_with_metadata<BaseAsset, QuoteAsset>(
        _pool: &mut Pool<BaseAsset, QuoteAsset>,
        _client_order_id: u64,
        _account_cap: &AccountCap,
        _quantity: u64,
        _clock: &Clock,
        _quote_coin: Coin<QuoteAsset>,
        _ctx: &mut TxContext,
    ): (Coin<BaseAsset>, Coin<QuoteAsset>, u64, vector<MatchedOrderMetadata<BaseAsset, QuoteAsset>>) {
        
        abort 1337
    }

    #[deprecated(note = b"Placing market order is deprecated in Deepbook V2. Please use Deepbook V3.")]
    /// Place a market order to the order book.
    public fun place_market_order<BaseAsset, QuoteAsset>(
        _pool: &mut Pool<BaseAsset, QuoteAsset>,
        _account_cap: &AccountCap,
        _client_order_id: u64,
        _quantity: u64,
        _is_bid: bool,
        _base_coin: Coin<BaseAsset>,
        _quote_coin: Coin<QuoteAsset>,
        _clock: &Clock,
        _ctx: &mut TxContext,
    ): (Coin<BaseAsset>, Coin<QuoteAsset>) {
        
        abort 1337
    }

    #[deprecated(note = b"Placing market order is deprecated in Deepbook V2. Please use Deepbook V3.")]
    public fun place_market_order_with_metadata<BaseAsset, QuoteAsset>(
        _pool: &mut Pool<BaseAsset, QuoteAsset>,
        _account_cap: &AccountCap,
        _client_order_id: u64,
        _quantity: u64,
        _is_bid: bool,
        _base_coin: Coin<BaseAsset>,
        _quote_coin: Coin<QuoteAsset>,
        _clock: &Clock,
        _ctx: &mut TxContext,
    ): (Coin<BaseAsset>, Coin<QuoteAsset>, vector<MatchedOrderMetadata<BaseAsset, QuoteAsset>>) {
        
        abort 1337
    }

    #[deprecated(note = b"Placing limit order is deprecated in Deepbook V2. Please use Deepbook V3.")]
    /// Place a limit order to the order book.
    /// Returns (base quantity filled, quote quantity filled, whether a maker order is being placed, order id of the maker order).
    /// When the limit order is not successfully placed, we return false to indicate that and also returns a meaningless order_id 0.
    /// When the limit order is successfully placed, we return true to indicate that and also the corresponding order_id.
    /// So please check that boolean value first before using the order id.
    public fun place_limit_order<BaseAsset, QuoteAsset>(
        _pool: &mut Pool<BaseAsset, QuoteAsset>,
        _client_order_id: u64,
        _price: u64,
        _quantity: u64,
        _self_matching_prevention: u8,
        _is_bid: bool,
        _expire_timestamp: u64, // Expiration timestamp in ms in absolute value inclusive.
        _restriction: u8,
        _clock: &Clock,
        _account_cap: &AccountCap,
        _ctx: &mut TxContext
    ): (u64, u64, bool, u64) {
        
        abort 1337
    }

    #[deprecated(note = b"Placing limit order is deprecated in Deepbook V2. Please use Deepbook V3.")]
    /// Place a limit order to the order book.
    /// Returns (base quantity filled, quote quantity filled, whether a maker order is being placed, order id of the maker order).
    /// When the limit order is not successfully placed, we return false to indicate that and also returns a meaningless order_id 0.
    /// When the limit order is successfully placed, we return true to indicate that and also the corresponding order_id.
    /// So please check that boolean value first before using the order id.
    public fun place_limit_order_with_metadata<BaseAsset, QuoteAsset>(
        _pool: &mut Pool<BaseAsset, QuoteAsset>,
        _client_order_id: u64,
        _price: u64,
        _quantity: u64,
        _self_matching_prevention: u8,
        _is_bid: bool,
        _expire_timestamp: u64, // Expiration timestamp in ms in absolute value inclusive.
        _restriction: u8,
        _clock: &Clock,
        _account_cap: &AccountCap,
        _ctx: &mut TxContext
    ): (u64, u64, bool, u64, vector<MatchedOrderMetadata<BaseAsset, QuoteAsset>>) {
        
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
            client_order_id: order.client_order_id,
            order_id: order.order_id,
            is_bid: order.is_bid,
            owner: order.owner,
            original_quantity: order.original_quantity,
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
        let owner = account_owner(account_cap);
        assert!(contains(&pool.usr_open_orders, owner), EInvalidUser);
        let usr_open_orders = borrow_mut(&mut pool.usr_open_orders, owner);
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
            owner
        );
        if (is_bid) {
            let (_, balance_locked) = clob_math::unsafe_mul_round(order.quantity, order.price);
            custodian::unlock_balance(&mut pool.quote_custodian, owner, balance_locked);
        } else {
            custodian::unlock_balance(&mut pool.base_custodian, owner, order.quantity);
        };
        emit_order_canceled<BaseAsset, QuoteAsset>(*object::uid_as_inner(&pool.id), &order);
    }

    fun remove_order(
        open_orders: &mut CritbitTree<TickLevel>,
        usr_open_orders: &mut LinkedTable<u64, u64>,
        tick_index: u64,
        order_id: u64,
        owner: address,
    ): Order {
        linked_table::remove(usr_open_orders, order_id);
        let tick_level = borrow_leaf_by_index(open_orders, tick_index);
        assert!(linked_table::contains(&tick_level.open_orders, order_id), EInvalidOrderId);
        let mut_tick_level = borrow_mut_leaf_by_index(open_orders, tick_index);
        let order = linked_table::remove(&mut mut_tick_level.open_orders, order_id);
        assert!(order.owner == owner, EUnauthorizedCancel);
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
        let owner = account_owner(account_cap);
        assert!(contains(&pool.usr_open_orders, owner), EInvalidUser);
        let usr_open_order_ids = table::borrow_mut(&mut pool.usr_open_orders, owner);
        let mut canceled_order_events = vector[];
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
                owner
            );
            if (is_bid) {
                let (_, balance_locked) = clob_math::unsafe_mul_round(order.quantity, order.price);
                custodian::unlock_balance(&mut pool.quote_custodian, owner, balance_locked);
            } else {
                custodian::unlock_balance(&mut pool.base_custodian, owner, order.quantity);
            };
            let canceled_order_event = AllOrdersCanceledComponent<BaseAsset, QuoteAsset> {
                client_order_id: order.client_order_id,
                order_id: order.order_id,
                is_bid: order.is_bid,
                owner: order.owner,
                original_quantity: order.original_quantity,
                base_asset_quantity_canceled: order.quantity,
                price: order.price
            };

            vector::push_back(&mut canceled_order_events, canceled_order_event);
        };

        if (!vector::is_empty(&canceled_order_events)) {
            event::emit(AllOrdersCanceled<BaseAsset, QuoteAsset> {
                pool_id,
                orders_canceled: canceled_order_events,
            });
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
        let owner = account_owner(account_cap);
        assert!(contains(&pool.usr_open_orders, owner), 0);
        let mut tick_index: u64 = 0;
        let mut tick_price: u64 = 0;
        let n_order = vector::length(&order_ids);
        let mut i_order = 0;
        let usr_open_orders = borrow_mut(&mut pool.usr_open_orders, owner);
        let mut canceled_order_events = vector[];

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
                owner
            );
            if (is_bid) {
                let (_is_round_down, balance_locked) = clob_math::unsafe_mul_round(order.quantity, order.price);
                custodian::unlock_balance(&mut pool.quote_custodian, owner, balance_locked);
            } else {
                custodian::unlock_balance(&mut pool.base_custodian, owner, order.quantity);
            };
            let canceled_order_event = AllOrdersCanceledComponent<BaseAsset, QuoteAsset> {
                client_order_id: order.client_order_id,
                order_id: order.order_id,
                is_bid: order.is_bid,
                owner: order.owner,
                original_quantity: order.original_quantity,
                base_asset_quantity_canceled: order.quantity,
                price: order.price
            };
            vector::push_back(&mut canceled_order_events, canceled_order_event);

            i_order = i_order + 1;
        };

        if (!vector::is_empty(&canceled_order_events)) {
            event::emit(AllOrdersCanceled<BaseAsset, QuoteAsset> {
                pool_id,
                orders_canceled: canceled_order_events,
            });
        };
    }

    /// Clean up expired orders
    /// Note that this function can reduce gas cost if orders
    /// with the same price are grouped together in the vector because we would not need the computation to find the tick_index.
    /// For example, if we have the following order_id to price mapping, {0: 100., 1: 200., 2: 100., 3: 200.}.
    /// Grouping order_ids like [0, 2, 1, 3] would make it the most gas efficient.
    /// Order owners should be the owner addresses from the account capacities which placed the orders,
    /// and they should correspond to the order IDs one by one.
    public fun clean_up_expired_orders<BaseAsset, QuoteAsset>(
        pool: &mut Pool<BaseAsset, QuoteAsset>,
        clock: &Clock,
        order_ids: vector<u64>,
        order_owners: vector<address>
    ) {
        let pool_id = *object::uid_as_inner(&pool.id);
        let now = clock::timestamp_ms(clock);
        let n_order = vector::length(&order_ids);
        assert!(n_order == vector::length(&order_owners), ENotEqual);
        let mut i_order = 0;
        let mut tick_index: u64 = 0;
        let mut tick_price: u64 = 0;
        let mut canceled_order_events = vector[];
        while (i_order < n_order) {
            let order_id = *vector::borrow(&order_ids, i_order);
            let owner = *vector::borrow(&order_owners, i_order);
            if (!table::contains(&pool.usr_open_orders, owner)) { continue };
            let usr_open_orders = borrow_mut(&mut pool.usr_open_orders, owner);
            if (!linked_table::contains(usr_open_orders, order_id)) { continue };
            let new_tick_price = *linked_table::borrow(usr_open_orders, order_id);
            let is_bid = order_is_bid(order_id);
            let open_orders = if (is_bid) { &mut pool.bids } else { &mut pool.asks };
            if (new_tick_price != tick_price) {
                tick_price = new_tick_price;
                let (tick_exists, new_tick_index) = find_leaf(
                    open_orders,
                    tick_price
                );
                assert!(tick_exists, EInvalidTickPrice);
                tick_index = new_tick_index;
            };
            let order = remove_order(open_orders, usr_open_orders, tick_index, order_id, owner);
            assert!(order.expire_timestamp < now, EInvalidExpireTimestamp);
            if (is_bid) {
                let (_is_round_down, balance_locked) = clob_math::unsafe_mul_round(order.quantity, order.price);
                custodian::unlock_balance(&mut pool.quote_custodian, owner, balance_locked);
            } else {
                custodian::unlock_balance(&mut pool.base_custodian, owner, order.quantity);
            };
            let canceled_order_event = AllOrdersCanceledComponent<BaseAsset, QuoteAsset> {
                client_order_id: order.client_order_id,
                order_id: order.order_id,
                is_bid: order.is_bid,
                owner: order.owner,
                original_quantity: order.original_quantity,
                base_asset_quantity_canceled: order.quantity,
                price: order.price
            };
            vector::push_back(&mut canceled_order_events, canceled_order_event);

            i_order = i_order + 1;
        };

        if (!vector::is_empty(&canceled_order_events)) {
            event::emit(AllOrdersCanceled<BaseAsset, QuoteAsset> {
                pool_id,
                orders_canceled: canceled_order_events,
            });
        };
    }

    public fun list_open_orders<BaseAsset, QuoteAsset>(
        pool: &Pool<BaseAsset, QuoteAsset>,
        account_cap: &AccountCap
    ): vector<Order> {
        let owner = account_owner(account_cap);
        let mut open_orders = vector::empty<Order>();
        if (!usr_open_orders_exist(pool, owner)) {
            return open_orders
        };
        let usr_open_order_ids = table::borrow(&pool.usr_open_orders, owner);
        let mut order_id = linked_table::front(usr_open_order_ids);
        while (!option::is_none(order_id)) {
            let order_price = *linked_table::borrow(usr_open_order_ids, *option::borrow(order_id));
            let tick_level =
                if (order_is_bid(*option::borrow(order_id))) borrow_leaf_by_key(&pool.bids, order_price)
                else borrow_leaf_by_key(&pool.asks, order_price);
            let order = linked_table::borrow(&tick_level.open_orders, *option::borrow(order_id));
            vector::push_back(&mut open_orders, Order {
                order_id: order.order_id,
                client_order_id: order.client_order_id,
                price: order.price,
                original_quantity: order.original_quantity,
                quantity: order.quantity,
                is_bid: order.is_bid,
                owner: order.owner,
                expire_timestamp: order.expire_timestamp,
                self_matching_prevention: order.self_matching_prevention
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
        let owner = account_owner(account_cap);
        let (base_avail, base_locked) = custodian::account_balance(&pool.base_custodian, owner);
        let (quote_avail, quote_locked) = custodian::account_balance(&pool.quote_custodian, owner);
        (base_avail, base_locked, quote_avail, quote_locked)
    }

    /// Query the market price of order book
    /// returns (best_bid_price, best_ask_price) if there exists
    /// bid/ask order in the order book, otherwise returns None
    public fun get_market_price<BaseAsset, QuoteAsset>(
        pool: &Pool<BaseAsset, QuoteAsset>
    ): (Option<u64>, Option<u64>){
        let bid_price = if (!critbit::is_empty(&pool.bids)) {
            let (result, _) = critbit::max_leaf(&pool.bids);
            option::some<u64>(result)
        } else {
            option::none<u64>()
        };
        let ask_price = if (!critbit::is_empty(&pool.asks)) {
            let (result, _) = critbit::min_leaf(&pool.asks);
            option::some<u64>(result)
        } else {
            option::none<u64>()
        };
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
        let mut price_vec = vector::empty<u64>();
        let mut depth_vec = vector::empty<u64>();
        if (critbit::is_empty(&pool.bids)) { return (price_vec, depth_vec) };
        let (price_low_, _) = critbit::min_leaf(&pool.bids);
        let (price_high_, _) = critbit::max_leaf(&pool.bids);

        // If price_low is greater than the highest element in the tree, we return empty
        if (price_low > price_high_) {
            return (price_vec, depth_vec)
        };

        if (price_low < price_low_) price_low = price_low_;
        if (price_high > price_high_) price_high = price_high_;
        price_low = critbit::find_closest_key(&pool.bids, price_low);
        price_high = critbit::find_closest_key(&pool.bids, price_high);
        while (price_low <= price_high) {
            let depth = get_level2_book_status(
                &pool.bids,
                price_low,
                clock::timestamp_ms(clock)
            );
            if (depth != 0) {
                vector::push_back(&mut price_vec, price_low);
                vector::push_back(&mut depth_vec, depth);
            };
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
        let mut price_vec = vector::empty<u64>();
        let mut depth_vec = vector::empty<u64>();
        if (critbit::is_empty(&pool.asks)) { return (price_vec, depth_vec) };
        let (price_low_, _) = critbit::min_leaf(&pool.asks);

        // Price_high is less than the lowest leaf in the tree then we return an empty array
        if (price_high < price_low_) {
            return (price_vec, depth_vec)
        };

        if (price_low < price_low_) price_low = price_low_;
        let (price_high_, _) = critbit::max_leaf(&pool.asks);
        if (price_high > price_high_) price_high = price_high_;
        price_low = critbit::find_closest_key(&pool.asks, price_low);
        price_high = critbit::find_closest_key(&pool.asks, price_high);
        while (price_low <= price_high) {
            let depth = get_level2_book_status(
                &pool.asks,
                price_low,
                clock::timestamp_ms(clock)
            );
            if (depth != 0) {
                vector::push_back(&mut price_vec, price_low);
                vector::push_back(&mut depth_vec, depth);
            };
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
        let owner = account_owner(account_cap);
        assert!(table::contains(&pool.usr_open_orders, owner), EInvalidUser);
        let usr_open_order_ids = table::borrow(&pool.usr_open_orders, owner);
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

    #[deprecated(note = b"Matching order is deprecated in Deepbook V2. Please use Deepbook V3.")]
    public fun matched_order_metadata_info<BaseAsset, QuoteAsset>(
        _matched_order_metadata: &MatchedOrderMetadata<BaseAsset, QuoteAsset>
    ) : ( ID, u64, bool, address, address, u64, u64, u64, u64) {
        
        abort 1337
    }

    // Methods for accessing pool data, used by the order_query package
    public fun asks<BaseAsset, QuoteAsset>(pool: &Pool<BaseAsset, QuoteAsset>): &CritbitTree<TickLevel> {
        &pool.asks
    }

    public fun bids<BaseAsset, QuoteAsset>(pool: &Pool<BaseAsset, QuoteAsset>): &CritbitTree<TickLevel> {
        &pool.bids
    }

    public fun tick_size<BaseAsset, QuoteAsset>(pool: &Pool<BaseAsset, QuoteAsset>): u64 {
        pool.tick_size
    }

    public fun maker_rebate_rate<BaseAsset, QuoteAsset>(pool: &Pool<BaseAsset, QuoteAsset>): u64 {
        pool.maker_rebate_rate
    }

    public fun taker_fee_rate<BaseAsset, QuoteAsset>(pool: &Pool<BaseAsset, QuoteAsset>): u64 {
        pool.taker_fee_rate
    }

    public fun pool_size<BaseAsset, QuoteAsset>(pool: &Pool<BaseAsset, QuoteAsset>): u64 {
        critbit::size(&pool.asks) + critbit::size(&pool.bids)
    }

    public fun open_orders(tick_level: &TickLevel): &LinkedTable<u64, Order> {
        &tick_level.open_orders
    }

    // Order Accessors

    public fun order_id(order: &Order): u64 {
        order.order_id
    }

    public fun tick_level(order: &Order): u64 {
        order.price
    }

    public fun original_quantity(order: &Order): u64 {
        order.original_quantity
    }

    public fun quantity(order: &Order): u64 {
        order.quantity
    }

    public fun is_bid(order: &Order): bool {
        order.is_bid
    }

    public fun owner(order: &Order): address {
        order.owner
    }

    public fun expire_timestamp(order: &Order): u64 {
        order.expire_timestamp
    }

    public fun quote_asset_trading_fees_value<BaseAsset, QuoteAsset>(pool: &Pool<BaseAsset, QuoteAsset>): u64 {
        balance::value(&pool.quote_asset_trading_fees)
    }

    public(package) fun clone_order(order: &Order): Order {
        Order {
            order_id: order.order_id,
            client_order_id: order.client_order_id,
            price: order.price,
            original_quantity: order.original_quantity,
            quantity: order.quantity,
            is_bid: order.is_bid,
            owner: order.owner,
            expire_timestamp: order.expire_timestamp,
            self_matching_prevention: order.self_matching_prevention
        }
    }
}

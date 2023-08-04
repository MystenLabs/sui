// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[allow(unused_field)]
/// [DEPRECATED]
/// This module is deprecated and is no longer functional. Use the `clob_v2`
/// module instead.
///
/// Legacy type definitions and public functions are kept due to package upgrade
/// constraints.
module deepbook::clob {
    use std::type_name::TypeName;
    use sui::balance::Balance;
    use sui::clock::Clock;
    use sui::coin::Coin;
    use sui::linked_table::LinkedTable;
    use sui::object::{UID, ID};
    use sui::sui::SUI;
    use sui::table::Table;
    use sui::tx_context::TxContext;

    use deepbook::critbit::CritbitTree;
    use deepbook::custodian::{Custodian, AccountCap};

    const EDeprecated: u64 = 1337;

    struct PoolCreated has copy, store, drop {
        pool_id: ID,
        base_asset: TypeName,
        quote_asset: TypeName,
        taker_fee_rate: u64,
        maker_rebate_rate: u64,
        tick_size: u64,
        lot_size: u64,
    }

    struct OrderPlacedV2<phantom BaseAsset, phantom QuoteAsset> has copy, store, drop {
        pool_id: ID,
        order_id: u64,
        is_bid: bool,
        owner: ID,
        base_asset_quantity_placed: u64,
        price: u64,
        expire_timestamp: u64
    }

    struct OrderCanceled<phantom BaseAsset, phantom QuoteAsset> has copy, store, drop {
        pool_id: ID,
        order_id: u64,
        is_bid: bool,
        owner: ID,
        base_asset_quantity_canceled: u64,
        price: u64
    }

    struct OrderFilledV2<phantom BaseAsset, phantom QuoteAsset> has copy, store, drop {
        pool_id: ID,
        order_id: u64,
        is_bid: bool,
        owner: ID,
        total_quantity: u64,
        base_asset_quantity_filled: u64,
        base_asset_quantity_remaining: u64,
        price: u64,
        taker_commission: u64,
        maker_rebates: u64
    }

    struct Order has store, drop {
        order_id: u64,
        price: u64,
        quantity: u64,
        is_bid: bool,
        owner: ID,
        expire_timestamp: u64,
    }

    struct TickLevel has store {
        price: u64,
        open_orders: LinkedTable<u64, Order>,
    }

    struct Pool<phantom BaseAsset, phantom QuoteAsset> has key {
        id: UID,
        bids: CritbitTree<TickLevel>,
        asks: CritbitTree<TickLevel>,
        next_bid_order_id: u64,
        next_ask_order_id: u64,
        usr_open_orders: Table<ID, LinkedTable<u64, u64>>,
        taker_fee_rate: u64,
        maker_rebate_rate: u64,
        tick_size: u64,
        lot_size: u64,
        base_custodian: Custodian<BaseAsset>,
        quote_custodian: Custodian<QuoteAsset>,
        creation_fee: Balance<SUI>,
        base_asset_trading_fees: Balance<BaseAsset>,
        quote_asset_trading_fees: Balance<QuoteAsset>,
    }

    public fun create_account(_ctx: &mut TxContext): AccountCap {
        abort EDeprecated
    }

    public fun create_pool<BaseAsset, QuoteAsset>(_a: u64, _b: u64, _c: Coin<SUI>, _ctx: &mut TxContext) {
        abort EDeprecated
    }

    public fun deposit_base<BaseAsset, QuoteAsset>(
        _pool: &mut Pool<BaseAsset, QuoteAsset>,
        _coin: Coin<BaseAsset>,
        _account_cap: &AccountCap
    ) {
        abort EDeprecated
    }

    public fun deposit_quote<BaseAsset, QuoteAsset>(
        _pool: &mut Pool<BaseAsset, QuoteAsset>,
        _coin: Coin<QuoteAsset>,
        _account_cap: &AccountCap
    ) {
        abort EDeprecated
    }

    public fun withdraw_base<BaseAsset, QuoteAsset>(
        _pool: &mut Pool<BaseAsset, QuoteAsset>,
        _quantity: u64,
        _account_cap: &AccountCap,
        _ctx: &mut TxContext
    ): Coin<BaseAsset> {
        abort EDeprecated
    }

    public fun withdraw_quote<BaseAsset, QuoteAsset>(
        _pool: &mut Pool<BaseAsset, QuoteAsset>,
        _quantity: u64,
        _account_cap: &AccountCap,
        _ctx: &mut TxContext
    ): Coin<QuoteAsset> {
        abort EDeprecated
    }

    public fun swap_exact_base_for_quote<BaseAsset, QuoteAsset>(
        _pool: &mut Pool<BaseAsset, QuoteAsset>,
        _quantity: u64,
        _base_coin: Coin<BaseAsset>,
        _quote_coin: Coin<QuoteAsset>,
        _clock: &Clock,
        _ctx: &mut TxContext,
    ): (Coin<BaseAsset>, Coin<QuoteAsset>, u64) {
        abort EDeprecated
    }

    public fun swap_exact_quote_for_base<BaseAsset, QuoteAsset>(
        _pool: &mut Pool<BaseAsset, QuoteAsset>,
        _quantity: u64,
        _clock: &Clock,
        _quote_coin: Coin<QuoteAsset>,
        _ctx: &mut TxContext,
    ): (Coin<BaseAsset>, Coin<QuoteAsset>, u64) {
        abort EDeprecated
    }

    public fun place_market_order<BaseAsset, QuoteAsset>(
        _pool: &mut Pool<BaseAsset, QuoteAsset>,
        _quantity: u64,
        _is_bid: bool,
        _base_coin: Coin<BaseAsset>,
        _quote_coin: Coin<QuoteAsset>,
        _clock: &Clock,
        _ctx: &mut TxContext,
    ): (Coin<BaseAsset>, Coin<QuoteAsset>) {
        abort EDeprecated
    }

    public fun place_limit_order<BaseAsset, QuoteAsset>(
        _pool: &mut Pool<BaseAsset, QuoteAsset>,
        _price: u64,
        _quantity: u64,
        _is_bid: bool,
        _restriction: u8,
        _clock: &Clock,
        _account_cap: &AccountCap,
        _ctx: &mut TxContext
    ): (u64, u64, bool, u64) {
        abort EDeprecated
    }

    public fun cancel_order<BaseAsset, QuoteAsset>(
        _pool: &mut Pool<BaseAsset, QuoteAsset>,
        _order_id: u64,
        _account_cap: &AccountCap
    ) {
        abort EDeprecated
    }

    public fun cancel_all_orders<BaseAsset, QuoteAsset>(
        _pool: &mut Pool<BaseAsset, QuoteAsset>,
        _account_cap: &AccountCap
    ) {
        abort EDeprecated
    }


    public fun batch_cancel_order<BaseAsset, QuoteAsset>(
        _pool: &mut Pool<BaseAsset, QuoteAsset>,
        _order_ids: vector<u64>,
        _account_cap: &AccountCap
    ) {
        abort EDeprecated
    }

    public fun list_open_orders<BaseAsset, QuoteAsset>(
        _pool: &Pool<BaseAsset, QuoteAsset>,
        _account_cap: &AccountCap
    ): vector<Order> {
        abort EDeprecated
    }

    public fun account_balance<BaseAsset, QuoteAsset>(
        _pool: &Pool<BaseAsset, QuoteAsset>,
        _account_cap: &AccountCap
    ): (u64, u64, u64, u64) {
        abort EDeprecated
    }

    public fun get_market_price<BaseAsset, QuoteAsset>(
        _pool: &Pool<BaseAsset, QuoteAsset>
    ): (u64, u64){
        abort EDeprecated
    }

    public fun get_level2_book_status_bid_side<BaseAsset, QuoteAsset>(
        _pool: &Pool<BaseAsset, QuoteAsset>,
        _price_low: u64,
        _price_high: u64,
        _clock: &Clock
    ): (vector<u64>, vector<u64>) {
        abort EDeprecated
    }

    public fun get_level2_book_status_ask_side<BaseAsset, QuoteAsset>(
        _pool: &Pool<BaseAsset, QuoteAsset>,
        _price_low: u64,
        _price_high: u64,
        _clock: &Clock
    ): (vector<u64>, vector<u64>) {
        abort EDeprecated
    }

    public fun get_order_status<BaseAsset, QuoteAsset>(
        _pool: &Pool<BaseAsset, QuoteAsset>,
        _order_id: u64,
        _account_cap: &AccountCap
    ): &Order {
        abort EDeprecated
    }

    /// DEPRECATED
    struct OrderPlaced<phantom BaseAsset, phantom QuoteAsset> has copy, store, drop {
        pool_id: ID,
        order_id: u64,
        is_bid: bool,
        owner: ID,
        base_asset_quantity_placed: u64,
        price: u64,
    }

    /// DEPRECATED
    struct OrderFilled<phantom BaseAsset, phantom QuoteAsset> has copy, store, drop {
        pool_id: ID,
        order_id: u64,
        is_bid: bool,
        owner: ID,
        total_quantity: u64,
        base_asset_quantity_filled: u64,
        base_asset_quantity_remaining: u64,
        price: u64
    }
}

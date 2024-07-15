// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Order module defines the order struct and its methods.
/// All order matching happens in this module.
module deepbook::order_info {
    // === Imports ===
    use sui::event;
    use deepbook::{
        math,
        order::{Self, Order},
        fill::Fill,
        balances::{Self, Balances},
        constants,
        deep_price::OrderDeepPrice,
    };

    // === Errors ===
    const EOrderInvalidPrice: u64 = 0;
    const EOrderBelowMinimumSize: u64 = 1;
    const EOrderInvalidLotSize: u64 = 2;
    const EInvalidExpireTimestamp: u64 = 3;
    const EInvalidOrderType: u64 = 4;
    const EPOSTOrderCrossesOrderbook: u64 = 5;
    const EFOKOrderCannotBeFullyFilled: u64 = 6;
    const EMarketOrderCannotBePostOnly: u64 = 7;
    const ESelfMatchingCancelTaker: u64 = 8;

    // === Structs ===
    /// OrderInfo struct represents all order information.
    /// This objects gets created at the beginning of the order lifecycle and
    /// gets updated until it is completed or placed in the book.
    /// It is returned at the end of the order lifecycle.
    public struct OrderInfo has store, drop {
        // ID of the pool
        pool_id: ID,
        // ID of the order within the pool
        order_id: u128,
        // ID of the account the order uses
        balance_manager_id: ID,
        // ID of the order defined by client
        client_order_id: u64,
        // Trader of the order
        trader: address,
        // Order type, NO_RESTRICTION, IMMEDIATE_OR_CANCEL, FILL_OR_KILL, POST_ONLY
        order_type: u8,
        // Self matching option,
        self_matching_option: u8,
        // Price, only used for limit orders
        price: u64,
        // Whether the order is a buy or a sell
        is_bid: bool,
        // Quantity (in base asset terms) when the order is placed
        original_quantity: u64,
        // Deep conversion used by the order
        order_deep_price: OrderDeepPrice,
        // Expiration timestamp in ms
        expire_timestamp: u64,
        // Quantity executed so far
        executed_quantity: u64,
        // Cumulative quote quantity executed so far
        cumulative_quote_quantity: u64,
        // Any partial fills
        fills: vector<Fill>,
        // Whether the fee is in DEEP terms
        fee_is_deep: bool,
        // Fees paid so far in base/quote/DEEP terms
        paid_fees: u64,
        // Epoch this order was placed
        epoch: u64,
        // Status of the order
        status: u8,
        // Is a market_order
        market_order: bool,
    }

    /// Emitted when a maker order is filled.
    public struct OrderFilled has copy, store, drop {
        pool_id: ID,
        maker_order_id: u128,
        taker_order_id: u128,
        maker_client_order_id: u64,
        taker_client_order_id: u64,
        price: u64,
        taker_is_bid: bool,
        base_quantity: u64,
        quote_quantity: u64,
        maker_balance_manager_id: ID,
        taker_balance_manager_id: ID,
        timestamp: u64,
    }

    /// Fills are emitted in batches of 100.
    public struct OrdersFilled has copy, store, drop {
        fills: vector<OrderFilled>,
    }

    /// Emitted when a maker order is canceled.
    public struct OrderCanceled<phantom BaseAsset, phantom QuoteAsset> has copy, store, drop {
        pool_id: ID,
        order_id: u128,
        client_order_id: u64,
        price: u64,
        is_bid: bool,
        base_asset_quantity_canceled: u64,
        timestamp: u64,
    }

    /// Emitted when a maker order is modified.
    public struct OrderModified<phantom BaseAsset, phantom QuoteAsset> has copy, store, drop {
        pool_id: ID,
        order_id: u128,
        client_order_id: u64,
        price: u64,
        is_bid: bool,
        new_quantity: u64,
        timestamp: u64,
    }

    /// Emitted when a maker order is injected into the order book.
    public struct OrderPlaced has copy, store, drop {
        balance_manager_id: ID,
        pool_id: ID,
        order_id: u128,
        client_order_id: u64,
        trader: address,
        price: u64,
        is_bid: bool,
        placed_quantity: u64,
        expire_timestamp: u64,
    }

    // === Public-View Functions ===
    public fun balance_manager_id(self: &OrderInfo): ID {
        self.balance_manager_id
    }

    public fun pool_id(self: &OrderInfo): ID {
        self.pool_id
    }

    public fun order_id(self: &OrderInfo): u128 {
        self.order_id
    }

    public fun client_order_id(self: &OrderInfo): u64 {
        self.client_order_id
    }

    public fun order_type(self: &OrderInfo): u8 {
        self.order_type
    }

    public fun self_matching_option(self: &OrderInfo): u8 {
        self.self_matching_option
    }

    public fun price(self: &OrderInfo): u64 {
        self.price
    }

    public fun is_bid(self: &OrderInfo): bool {
        self.is_bid
    }

    public fun original_quantity(self: &OrderInfo): u64 {
        self.original_quantity
    }

    public fun order_deep_price(self: &OrderInfo): OrderDeepPrice {
        self.order_deep_price
    }

    public fun executed_quantity(self: &OrderInfo): u64 {
        self.executed_quantity
    }

    public fun cumulative_quote_quantity(self: &OrderInfo): u64 {
        self.cumulative_quote_quantity
    }

    public fun paid_fees(self: &OrderInfo): u64 {
        self.paid_fees
    }

    public fun epoch(self: &OrderInfo): u64 {
        self.epoch
    }

    public fun fee_is_deep(self: &OrderInfo): bool {
        self.fee_is_deep
    }

    public fun status(self: &OrderInfo): u8 {
        self.status
    }

    public fun expire_timestamp(self: &OrderInfo): u64 {
        self.expire_timestamp
    }

    public fun fills(self: &OrderInfo): vector<Fill> {
        self.fills
    }

    // === Public-Package Functions ===
    public(package) fun new(
        pool_id: ID,
        balance_manager_id: ID,
        client_order_id: u64,
        trader: address,
        order_type: u8,
        self_matching_option: u8,
        price: u64,
        quantity: u64,
        is_bid: bool,
        fee_is_deep: bool,
        epoch: u64,
        expire_timestamp: u64,
        order_deep_price: OrderDeepPrice,
        market_order: bool,
    ): OrderInfo {
        OrderInfo {
            pool_id,
            order_id: 0,
            balance_manager_id,
            client_order_id,
            trader,
            order_type,
            self_matching_option,
            price,
            is_bid,
            original_quantity: quantity,
            order_deep_price,
            expire_timestamp,
            executed_quantity: 0,
            cumulative_quote_quantity: 0,
            fills: vector[],
            fee_is_deep,
            epoch,
            paid_fees: 0,
            status: constants::live(),
            market_order,
        }
    }

    public(package) fun market_order(self: &OrderInfo): bool {
        self.market_order
    }

    public(package) fun set_order_id(self: &mut OrderInfo, order_id: u128) {
        self.order_id = order_id;
    }

    public(package) fun set_paid_fees(self: &mut OrderInfo, paid_fees: u64) {
        self.paid_fees = paid_fees;
    }

    public(package) fun add_fill(self: &mut OrderInfo, fill: Fill) {
        self.fills.push_back(fill);
    }

    /// Given a partially filled `OrderInfo`, the taker fee and maker fee, for the user
    /// placing the order, calculate all of the balances that need to be settled and
    /// the balances that are owed. The executed quantity is multiplied by the taker_fee
    /// and the remaining quantity is multiplied by the maker_fee to get the DEEP fee.
    public(package) fun calculate_partial_fill_balances(
        self: &mut OrderInfo,
        taker_fee: u64,
        maker_fee: u64,
    ): (Balances, Balances) {
        let taker_deep_in = math::mul(
            taker_fee,
            self
                .order_deep_price
                .deep_quantity(
                    self.executed_quantity,
                    self.cumulative_quote_quantity,
                ),
        );
        self.paid_fees = taker_deep_in;

        let mut settled_balances = balances::new(0, 0, 0);
        let mut owed_balances = balances::new(0, 0, 0);
        owed_balances.add_deep(taker_deep_in);

        if (self.is_bid) {
            settled_balances.add_base(self.executed_quantity);
            owed_balances.add_quote(self.cumulative_quote_quantity);
        } else {
            settled_balances.add_quote(self.cumulative_quote_quantity);
            owed_balances.add_base(self.executed_quantity);
        };

        let remaining_quantity = self.remaining_quantity();
        if (remaining_quantity > 0 && !(self.order_type() == constants::immediate_or_cancel())) {
            let maker_deep_in = math::mul(
                maker_fee,
                self
                    .order_deep_price
                    .deep_quantity(
                        remaining_quantity,
                        math::mul(remaining_quantity, self.price()),
                    ),
            );
            owed_balances.add_deep(maker_deep_in);
            if (self.is_bid) {
                owed_balances.add_quote(math::mul(remaining_quantity, self.price()));
            } else {
                owed_balances.add_base(remaining_quantity);
            };
        };

        (settled_balances, owed_balances)
    }

    /// `OrderInfo` is converted to an `Order` before being injected into the order book.
    /// This is done to save space in the order book. Order contains the minimum
    /// information required to match orders.
    public(package) fun to_order(self: &OrderInfo): Order {
        order::new(
            self.order_id,
            self.balance_manager_id,
            self.client_order_id,
            self.remaining_quantity(),
            self.fee_is_deep,
            self.order_deep_price,
            self.epoch,
            self.status,
            self.expire_timestamp,
        )
    }

    /// Validates that the initial order created meets the pool requirements.
    public(package) fun validate_inputs(
        order_info: &OrderInfo,
        tick_size: u64,
        min_size: u64,
        lot_size: u64,
        timestamp: u64,
    ) {
        assert!(order_info.original_quantity >= min_size, EOrderBelowMinimumSize);
        assert!(order_info.original_quantity % lot_size == 0, EOrderInvalidLotSize);
        assert!(order_info.expire_timestamp >= timestamp, EInvalidExpireTimestamp);
        assert!(
            order_info.order_type >= constants::no_restriction() &&
            order_info.order_type <= constants::max_restriction(),
            EInvalidOrderType,
        );
        if (order_info.market_order) {
            assert!(order_info.order_type != constants::post_only(), EMarketOrderCannotBePostOnly);
            return
        };
        assert!(
            order_info.price >= constants::min_price() &&
            order_info.price <= constants::max_price(),
            EOrderInvalidPrice,
        );
        assert!(order_info.price % tick_size == 0, EOrderInvalidPrice);
    }

    /// Assert order types after partial fill against the order book.
    public(package) fun assert_execution(self: &mut OrderInfo): bool {
        if (self.order_type == constants::post_only()) {
            assert!(self.executed_quantity == 0, EPOSTOrderCrossesOrderbook)
        };
        if (self.order_type == constants::fill_or_kill()) {
            assert!(self.executed_quantity == self.original_quantity, EFOKOrderCannotBeFullyFilled)
        };
        if (self.order_type == constants::immediate_or_cancel()) {
            if (self.remaining_quantity() > 0) {
                self.status = constants::canceled();
            } else {
                self.status = constants::filled();
            };

            return true
        };

        if (self.remaining_quantity() == 0) {
            self.status = constants::filled();

            return true
        };

        false
    }

    /// Returns the remaining quantity for the order.
    public(package) fun remaining_quantity(self: &OrderInfo): u64 {
        self.original_quantity - self.executed_quantity
    }

    /// Returns true if two opposite orders are overlapping in price.
    public(package) fun can_match(self: &OrderInfo, order: &Order): bool {
        let maker_price = order.price();

        (
            self.original_quantity - self.executed_quantity > 0 &&
            (
                self.is_bid && self.price >= maker_price ||
                !self.is_bid && self.price <= maker_price,
            ),
        )
    }

    /// Matches an `OrderInfo` with an `Order` from the book. Appends a `Fill` to fills.
    /// If the book order is expired, the `Fill` will have the expired flag set to true.
    /// Funds for the match or an expired order are returned to the maker as settled.
    public(package) fun match_maker(self: &mut OrderInfo, maker: &mut Order, timestamp: u64): bool {
        if (!self.can_match(maker)) return false;

        if (self.self_matching_option() == constants::cancel_taker()) {
            assert!(
                maker.balance_manager_id() != self.balance_manager_id(),
                ESelfMatchingCancelTaker,
            );
        };
        let expire_maker = self.self_matching_option() == constants::cancel_maker() &&
        maker.balance_manager_id() == self.balance_manager_id();
        let fill = maker.generate_fill(
            timestamp,
            self.remaining_quantity(),
            self.is_bid,
            expire_maker,
        );
        self.fills.push_back(fill);
        if (fill.expired()) return true;

        self.executed_quantity = self.executed_quantity + fill.base_quantity();
        self.cumulative_quote_quantity = self.cumulative_quote_quantity + fill.quote_quantity();
        self.status = constants::partially_filled();
        if (self.remaining_quantity() == 0) self.status = constants::filled();

        true
    }

    /// Emit all fills for this order in a vector of `OrderFilled` events.
    /// To avoid DOS attacks, 100 fills are emitted at a time. Up to 10,000
    /// fills can be emitted in a single call.
    public(package) fun emit_orders_filled(
        self: &OrderInfo,
        timestamp: u64,
    ) {
        if (self.fills.is_empty()) return;

        let mut orders_filled = vector[];
        let mut i = 0;
        let mut j = 0;
        while (i < self.fills.length() && j < 5) {
            let fill = &self.fills[i];
            orders_filled.push_back(self.order_filled_from_fill(fill, timestamp));
            if (i % 5 == 0) {
                event::emit(OrdersFilled { fills: orders_filled });
                orders_filled = vector[];
                j = j + 1;
            };
            i = i + 1;
        };

        event::emit(OrdersFilled { fills: orders_filled })
    }

    public(package) fun emit_order_placed(self: &OrderInfo) {
        event::emit(OrderPlaced {
            balance_manager_id: self.balance_manager_id,
            pool_id: self.pool_id,
            order_id: self.order_id,
            client_order_id: self.client_order_id,
            is_bid: self.is_bid,
            trader: self.trader,
            placed_quantity: self.remaining_quantity(),
            price: self.price,
            expire_timestamp: self.expire_timestamp,
        });
    }

    // === Private Functions ===
    fun order_filled_from_fill(
        self: &OrderInfo,
        fill: &Fill,
        timestamp: u64,
    ): OrderFilled {
        OrderFilled {
            pool_id: self.pool_id,
            maker_order_id: fill.maker_order_id(),
            taker_order_id: self.order_id,
            maker_client_order_id: fill.maker_client_order_id(),
            taker_client_order_id: self.client_order_id,
            price: fill.execution_price(),
            taker_is_bid: self.is_bid,
            base_quantity: fill.base_quantity(),
            quote_quantity: fill.quote_quantity(),
            maker_balance_manager_id: fill.balance_manager_id(),
            taker_balance_manager_id: self.balance_manager_id,
            timestamp,
        }
    }
}

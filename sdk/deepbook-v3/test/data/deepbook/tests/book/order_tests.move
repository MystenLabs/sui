// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module deepbook::order_tests {
    use sui::{
        test_scenario::{next_tx, begin, end},
        test_utils::assert_eq,
        object::id_from_address,
    };
    use deepbook::{
        order::{Self, Order},
        utils,
        balances,
        constants,
        deep_price::Self,
    };

    const OWNER: address = @0xF;
    const ALICE: address = @0xA;

    #[test]
    // Maker has a sell order of 15 at $10. Gets matched for 5.
    fun generate_fill_partial_fill_ok() {
        let mut test = begin(OWNER);

        test.next_tx(ALICE);
        let price = 15 * constants::usdc_unit();
        let quantity = 10 * constants::sui_unit();
        let is_bid = false;
        let mut order = create_order_base(price, quantity, is_bid);

        let fill = order.generate_fill(0, 5 * constants::sui_unit(), true, false);
        assert!(!fill.expired(), 0);
        assert!(!fill.completed(), 0);
        assert!(fill.base_quantity() == 5 * constants::sui_unit(), 0);
        assert!(fill.taker_is_bid(), 0);
        assert!(fill.quote_quantity() == 75 * constants::usdc_unit(), 0); // 5 * $15 = $75
        assert_eq(fill.get_settled_maker_quantities(), balances::new(0, 75 * constants::usdc_unit(), 0));

        assert!(order.status() == constants::partially_filled(), 0);
        assert!(order.filled_quantity() == 5 * constants::sui_unit(), 0);

        test.end();
    }

    #[test]
    // Maker has a sell order of 0.1 at $111.11. Gets matched for 0.1.
    fun generate_fill_full_fill_ok() {
        let mut test = begin(OWNER);

        test.next_tx(ALICE);
        let price = 111_110_000;
        let quantity = 1 * constants::sui_unit() / 10;
        let is_bid = false;
        let mut order = create_order_base(price, quantity, is_bid);

        let fill = order.generate_fill(0, 1 * constants::sui_unit() / 10, true, false);
        assert!(!fill.expired(), 0);
        assert!(fill.completed(), 0);
        assert!(fill.base_quantity() == 1 * constants::sui_unit() / 10, 0);
        assert!(fill.taker_is_bid(), 0);
        assert!(fill.quote_quantity() == 11_111_000, 0); // 0.1 * $111.11 = $11.111
        assert_eq(fill.get_settled_maker_quantities(), balances::new(0, 11_111_000, 0));

        assert!(order.status() == constants::filled(), 0);
        assert!(order.filled_quantity() == 1 * constants::sui_unit() / 10, 0);

        test.end();
    }

    #[test]
    // Maker has a buy order of 1919 at $1.19. Gets matched for 0.01.
    fun generate_fill_partial_fill_ok_bid() {
        let mut test = begin(OWNER);

        test.next_tx(ALICE);
        let price = 1_190_000;
        let quantity = 1919 * constants::sui_unit();
        let is_bid = true;
        let mut order = create_order_base(price, quantity, is_bid);

        let fill = order.generate_fill(0, 1 * constants::sui_unit() / 100, false, false);
        assert!(!fill.expired(), 0);
        assert!(!fill.completed(), 0);
        assert!(fill.base_quantity() == 1 * constants::sui_unit() / 100, 0);
        assert!(!fill.taker_is_bid(), 0);
        assert!(fill.quote_quantity() == 11_900, 0); // 0.01 * $1.19 = $0.0119
        assert_eq(fill.get_settled_maker_quantities(), balances::new(1 * constants::sui_unit() / 100, 0, 0));

        assert!(order.status() == constants::partially_filled(), 0);
        assert!(order.filled_quantity() == 1 * constants::sui_unit() / 100, 0);

        test.end();
    }

    #[test]
    // Maker has a sell of 10 at $10 but taker is same as maker, self match option to expire.
    // Original base amount is returned to maker.
    fun generate_fill_self_match_expire_ok() {
        let mut test = begin(OWNER);

        test.next_tx(ALICE);
        let price = 10 * constants::usdc_unit();
        let quantity = 10 * constants::sui_unit();
        let is_bid = false;
        let mut order = create_order_base(price, quantity, is_bid);

        let fill = order.generate_fill(0, 10 * constants::sui_unit(), true, true);
        assert!(fill.expired(), 0);
        assert!(!fill.completed(), 0);
        assert!(fill.base_quantity() == 10 * constants::sui_unit(), 0);
        assert!(fill.quote_quantity() == 0, 0);
        assert_eq(fill.get_settled_maker_quantities(), balances::new(10 * constants::sui_unit(), 0, 0));

        assert!(order.status() == constants::expired(), 0);
        assert!(order.filled_quantity() == 0, 0);

        test.end();
    }

    #[test]
    // Maker has a buy order of 10 at $10 but is expired.
    // Original quote amount is returned to maker.
    fun generate_fill_expired_ok() {
        let mut test = begin(OWNER);

        test.next_tx(ALICE);
        let price = 10 * constants::usdc_unit();
        let quantity = 10 * constants::sui_unit();
        let is_bid = true;
        let deep_per_asset = 1 * constants::float_scaling();
        let order_id = 1;
        let balance_manager_id = id_from_address(ALICE);
        let epoch = 1;
        let expire_timestamp = test.ctx().epoch_timestamp_ms();
        let conversion_is_base = true;
        let mut order = create_order(
            price,
            quantity,
            is_bid,
            order_id,
            balance_manager_id,
            deep_per_asset,
            conversion_is_base,
            epoch,
            expire_timestamp,
        );

        let fill = order.generate_fill(test.ctx().epoch_timestamp_ms() + 1, 10 * constants::sui_unit(), false, false);
        assert!(fill.expired(), 0);
        assert!(!fill.completed(), 0);
        assert!(fill.base_quantity() == 10 * constants::sui_unit(), 0);
        assert!(fill.quote_quantity() == 0, 0);
        assert_eq(fill.get_settled_maker_quantities(), balances::new(0, 100 * constants::usdc_unit(), 0));

        assert!(order.status() == constants::expired(), 0);
        assert!(order.filled_quantity() == 0, 0);

        test.end();
    }

    #[test]
    // Start with quantity 10, modify it to 5, then fill 1, then modify it to 2.
    fun modify_ok() {
        let mut test = begin(OWNER);

        test.next_tx(ALICE);
        let price = 10 * constants::usdc_unit();
        let quantity = 10 * constants::sui_unit();
        let is_bid = false;
        let mut order = create_order_base(price, quantity, is_bid);
        let ts = order.expire_timestamp();

        let new_quantity = 5 * constants::sui_unit();
        order.modify(new_quantity, ts);
        assert!(order.quantity() == 5 * constants::sui_unit(), 0);
        assert!(order.filled_quantity() == 0, 0);
        assert!(order.status() == constants::live(), 0);

        order.generate_fill(0, 1 * constants::sui_unit(), true, false);
        assert!(order.quantity() == 5 * constants::sui_unit(), 0);
        assert!(order.filled_quantity() == 1 * constants::sui_unit(), 0);
        assert!(order.status() == constants::partially_filled(), 0);

        let new_quantity = 2 * constants::sui_unit();
        order.modify(new_quantity, ts);
        assert!(order.quantity() == 2 * constants::sui_unit(), 0);
        assert!(order.filled_quantity() == 1 * constants::sui_unit(), 0);
        assert!(order.status() == constants::partially_filled(), 0);

        order.generate_fill(0, 1 * constants::sui_unit(), true, false);
        assert!(order.quantity() == 2 * constants::sui_unit(), 0);
        assert!(order.filled_quantity() == 2 * constants::sui_unit(), 0);
        assert!(order.status() == constants::filled(), 0);

        test.end();
    }

    #[test, expected_failure(abort_code = order::EInvalidNewQuantity)]
    // Start with quantity 10, reduce it by 1 10 times.
    fun modify_invalid_quantity_e() {
        let mut test = begin(OWNER);

        test.next_tx(ALICE);
        let price = 10 * constants::usdc_unit();
        let quantity = 10 * constants::sui_unit();
        let is_bid = false;
        let mut order = create_order_base(price, quantity, is_bid);
        let ts = order.expire_timestamp();

        let new_quantity = 9 * constants::sui_unit();
        order.modify(new_quantity, ts);
        assert!(order.quantity() == new_quantity, 0);
        let new_quantity = 8 * constants::sui_unit();
        order.modify(new_quantity, ts);
        assert!(order.quantity() == new_quantity, 0);
        let new_quantity = 7 * constants::sui_unit();
        order.modify(new_quantity, ts);
        assert!(order.quantity() == new_quantity, 0);
        let new_quantity = 6 * constants::sui_unit();
        order.modify(new_quantity, ts);
        assert!(order.quantity() == new_quantity, 0);
        let new_quantity = 5 * constants::sui_unit();
        order.modify(new_quantity, ts);
        assert!(order.quantity() == new_quantity, 0);
        let new_quantity = 4 * constants::sui_unit();
        order.modify(new_quantity, ts);
        assert!(order.quantity() == new_quantity, 0);
        let new_quantity = 3 * constants::sui_unit();
        order.modify(new_quantity, ts);
        assert!(order.quantity() == new_quantity, 0);
        let new_quantity = 2 * constants::sui_unit();
        order.modify(new_quantity, ts);
        assert!(order.quantity() == new_quantity, 0);
        let new_quantity = 1 * constants::sui_unit();
        order.modify(new_quantity, ts);
        assert!(order.quantity() == new_quantity, 0);
        let new_quantity = 0 * constants::sui_unit();
        order.modify(new_quantity, ts);

        abort(0)
    }

    #[test, expected_failure(abort_code = order::EInvalidNewQuantity)]
    fun modify_quantity_too_high_e() {
        let mut test = begin(OWNER);

        test.next_tx(ALICE);
        let price = 10 * constants::usdc_unit();
        let quantity = 10 * constants::sui_unit();
        let is_bid = false;
        let mut order = create_order_base(price, quantity, is_bid);
        let ts = order.expire_timestamp();

        let new_quantity = 10 * constants::sui_unit();
        order.modify(new_quantity, ts);

        abort(0)
    }

    #[test, expected_failure(abort_code = order::EOrderExpired)]
    fun modify_expired_e() {
        let mut test = begin(OWNER);

        test.next_tx(ALICE);
        let price = 10 * constants::usdc_unit();
        let quantity = 10 * constants::sui_unit();
        let is_bid = true;
        let deep_per_asset = 1 * constants::float_scaling();
        let order_id = 1;
        let balance_manager_id = id_from_address(ALICE);
        let epoch = 1;
        let expire_timestamp = test.ctx().epoch_timestamp_ms() + 1000;
        let conversion_is_base = true;
        let mut order = create_order(
            price,
            quantity,
            is_bid,
            order_id,
            balance_manager_id,
            deep_per_asset,
            conversion_is_base,
            epoch,
            expire_timestamp,
        );

        let new_quantity = 5 * constants::sui_unit();
        order.modify(new_quantity, expire_timestamp + 1);

        abort(0)
    }

    #[test_only]
    public fun create_order_base (
        price: u64,
        quantity: u64,
        is_bid: bool,
    ): Order {
        let deep_per_asset = 1 * constants::float_scaling();
        let order_id = 1;
        let balance_manager_id = id_from_address(ALICE);
        let epoch = 1;
        let expire_timestamp = constants::max_u64();
        let conversion_is_base = true;

        create_order(
            price,
            quantity,
            is_bid,
            order_id,
            balance_manager_id,
            deep_per_asset,
            conversion_is_base,
            epoch,
            expire_timestamp,
        )
    }

    #[test_only]
    public fun create_order(
        price: u64,
        quantity: u64,
        is_bid: bool,
        order_id: u64,
        balance_manager_id: ID,
        deep_per_asset: u64,
        conversion_is_base: bool,
        epoch: u64,
        expire_timestamp: u64,
    ): Order {
        let order_id = utils::encode_order_id(is_bid, price, order_id);

        order::new(
            order_id,
            balance_manager_id,
            1,
            quantity,
            constants::fee_is_deep(),
            deep_price::new_order_deep_price(conversion_is_base, deep_per_asset),
            epoch,
            constants::live(),
            expire_timestamp,
        )
    }
}

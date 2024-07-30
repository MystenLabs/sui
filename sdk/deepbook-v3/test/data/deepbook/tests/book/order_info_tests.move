// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module deepbook::order_info_tests {
    use sui::{
        test_scenario::{next_tx, begin, end},
        test_utils::assert_eq,
        object::id_from_address,
    };
    use deepbook::{
        order_info::{Self, OrderInfo},
        utils,
        balances,
        constants,
        deep_price
    };

    const OWNER: address = @0xF;
    const ALICE: address = @0xA;
    const BOB: address = @0xB;

    #[test]
    // Placing a bid order with quantity 1 at price $1. No fill.
    // No taker fees, so maker fees should apply to entire quantity.
    // Since its a bid, we should be required to transfer 1 USDC into the pool.
    fun calculate_partial_fill_balances_ok() {
        let mut test = begin(OWNER);

        test.next_tx(ALICE);
        let price = 1 * constants::usdc_unit();
        let quantity = 1 * constants::sui_unit();
        let mut order_info = create_order_info_base(ALICE, price, quantity, true, test.ctx().epoch());
        let (settled, owed) = order_info.calculate_partial_fill_balances(constants::taker_fee(), constants::maker_fee());

        assert_eq(settled, balances::new(0, 0, 0));
        assert_eq(owed, balances::new(0, 1 * constants::usdc_unit(), 500_000)); // 5 bps of 1 SUI paid in DEEP

        end(test);
    }

    #[test]
    // Placing a bid order with quantity 10 at price $1.234. No fill.
    // No taker fees, so maker fees should apply to entire quantity.
    // Since its a bid, we should be required to transfer 1 USDC into the pool.
    fun calculate_partial_fill_balances_precision_ok() {
        let mut test = begin(OWNER);

        test.next_tx(ALICE);
        let price = 1_234_000;
        let quantity = 10 * constants::sui_unit();
        let mut order_info = create_order_info_base(ALICE, price, quantity, true, test.ctx().epoch());
        let (settled, owed) = order_info.calculate_partial_fill_balances(constants::taker_fee(), constants::maker_fee());

        assert_eq(settled, balances::new(0, 0, 0));
        assert_eq(owed, balances::new(0, 12_340_000, 5_000_000)); // 5 bps of 10 SUI paid in DEEP

        end(test);
    }

    #[test]
    // Placing a bid order with quantity 10.86 at price $1.234. No fill.
    fun calculate_partial_fill_balances_precision2_ok() {
        let mut test = begin(OWNER);

        test.next_tx(ALICE);
        let price = 1_234_000;
        let quantity = 10_860_000_000;
        let mut order_info = create_order_info_base(ALICE, price, quantity, true, test.ctx().epoch());
        let (settled, owed) = order_info.calculate_partial_fill_balances(constants::taker_fee(), constants::maker_fee());

        assert_eq(settled, balances::new(0, 0, 0));
        // USDC owed = 1.234 * 10.86 = 13.40124 = 13401240
        // DEEP owed = 10.86 * 0.0005 = 0.00543 = 5430000 (9 decimals in DEEP)
        assert_eq(owed, balances::new(0, 13401240, 5430000));

        end(test);
    }

    #[test]
    // Place an ask order with quantity 655.36 at price $19.32. No fill.
    fun calculate_partial_fill_balances_ask_no_fill_ok() {
        let mut test = begin(OWNER);

        test.next_tx(ALICE);
        let price = 19_320_000;
        let quantity = 655_360_000_000;
        let mut order_info = create_order_info_base(ALICE, price, quantity, false, test.ctx().epoch());
        let (settled, owed) = order_info.calculate_partial_fill_balances(constants::taker_fee(), constants::maker_fee());

        assert_eq(settled, balances::new(0, 0, 0));
        // Since its an ask, transfer quantity amount worth of base token.
        // DEEP owed = 655.36 * 0.0005 = 0.32768 = 327680000 (9 decimals in DEEP)
        assert_eq(owed, balances::new(655_360_000_000, 0, 327_680_000));

        end(test);
    }

    #[test]
    // Taker: bid order with quantity 10 at price $5
    // Maker: ask order with quantity 5 at price $5
    fun match_maker_partial_fill_bid_ok() {
        let mut test = begin(OWNER);

        test.next_tx(ALICE);
        let price = 5 * constants::usdc_unit();
        let taker_quantity = 10 * constants::sui_unit();
        let maker_quantity = 5 * constants::sui_unit();
        let mut order_info = create_order_info_base(ALICE, price, taker_quantity, true, test.ctx().epoch());
        let mut maker_order = create_order_info_base(BOB, price, maker_quantity, false, test.ctx().epoch()).to_order();
        let has_next = order_info.match_maker(&mut maker_order, 0);
        assert!(has_next, 0);
        assert!(order_info.fills().length() == 1, 0);
        assert!(order_info.executed_quantity() == 5 * constants::sui_unit(), 0);
        assert!(order_info.cumulative_quote_quantity() == 25 * constants::usdc_unit(), 0);
        assert!(order_info.status() == constants::partially_filled(), 0);
        assert!(order_info.remaining_quantity() == 5 * constants::sui_unit(), 0);

        end(test);
    }

    #[test]
    // Taker: bid order with quantity 111 at price $4
    // Maker: ask order with quantity 38.13 at price $3.89
    fun match_maker_partial_fill_ask_ok() {
        let mut test = begin(OWNER);

        test.next_tx(ALICE);
        let price = 4 * constants::usdc_unit();
        let taker_quantity = 111 * constants::sui_unit();
        let maker_quantity = 38_130_000_000;
        let mut order_info = create_order_info_base(ALICE, price, taker_quantity, true, test.ctx().epoch());
        let mut maker_order = create_order_info_base(BOB, 3_890_000, maker_quantity, false, test.ctx().epoch()).to_order();
        let has_next = order_info.match_maker(&mut maker_order, 0);
        assert!(has_next, 0);
        assert!(order_info.fills().length() == 1, 0);
        assert!(order_info.executed_quantity() == 38_130_000_000, 0);
        // 38.13 * 3.89 = 148.3257 = 148325700
        assert!(order_info.cumulative_quote_quantity() == 148_325_700, 0);
        assert!(order_info.status() == constants::partially_filled(), 0);
        assert!(order_info.remaining_quantity() == 72_870_000_000, 0);

        end(test);
    }

    #[test]
    // Taker: ask order with quantity 10 at price $1
    // Maker1: bid order with quantity 1.001001 at price $1.001
    // Maker2: bid order with quantity 1 at price $1
    fun match_maker_multiple_ask_ok() {
        let mut test = begin(OWNER);

        test.next_tx(ALICE);
        let price = 1 * constants::usdc_unit();
        let taker_quantity = 10 * constants::sui_unit();
        let maker1_quantity = 1_001_001_000;
        let maker2_quantity = 1 * constants::sui_unit();
        let mut order_info = create_order_info_base(ALICE, price, taker_quantity, false, test.ctx().epoch());
        let mut maker_order1 = create_order_info_base(BOB, 1_001_000, maker1_quantity, true, test.ctx().epoch()).to_order();
        // quantity matched = 1.001001, taker fee = 0.001 = 0.001001001
        order_info.match_maker(&mut maker_order1, 0);
        // quantity matched = 1, taker fee = 0.001 = 0.001
        let mut maker_order2 = create_order_info_base(BOB, price, maker2_quantity, true, test.ctx().epoch()).to_order();
        order_info.match_maker(&mut maker_order2, 0);
        // remaining quantity = 10 - 1 - 1.001001 = 7.998999
        // maker fee = 7.998999 * 0.0005 = 0.0039994995, rounded down 0.003999499
        // total fee = 0.001001001 + 0.001 + 0.003999499 = 0.0060005 = 6000500
        let (settled, owed) = order_info.calculate_partial_fill_balances(constants::taker_fee(), constants::maker_fee());

        assert_eq(settled, balances::new(0, 2_002_002, 0));
        assert_eq(owed, balances::new(10_000_000_000, 0, 6_000_500));

        end(test);
    }

    #[test]
    // Taker: bid order with quantity 10 at price $5
    // Maker: ask order with quantity 50 at price $5
    fun match_maker_full_fill_ok() {
        let mut test = begin(OWNER);

        test.next_tx(ALICE);
        let price = 5 * constants::usdc_unit();
        let taker_quantity = 10 * constants::sui_unit();
        let maker_quantity = 50 * constants::sui_unit();
        let mut order_info = create_order_info_base(ALICE, price, taker_quantity, true, test.ctx().epoch());
        let mut maker_order = create_order_info_base(BOB, price, maker_quantity, false, test.ctx().epoch()).to_order();
        let has_next = order_info.match_maker(&mut maker_order, 0);
        assert!(has_next, 0);
        assert!(order_info.fills().length() == 1, 0);
        assert!(order_info.executed_quantity() == 10 * constants::sui_unit(), 0);
        assert!(order_info.cumulative_quote_quantity() == 50 * constants::usdc_unit(), 0);
        assert!(order_info.status() == constants::filled(), 0);
        assert!(order_info.remaining_quantity() == 0, 0);

        end(test);
    }

    #[test]
    // Place a bid order with quantity 131.11 at price $1900. Partial fill of 100 at price $1813.05.
    fun calculate_partial_fill_balances_bid_partial_fill_ok() {
        let mut test = begin(OWNER);

        test.next_tx(ALICE);
        let price = 1_900_000_000;
        let maker_price = 1_813_050_000;
        let quantity = 131_110_000_000;
        let mut order_info = create_order_info_base(ALICE, price, quantity, true, test.ctx().epoch());
        let mut maker_order = create_order_info_base(BOB, maker_price, 100 * constants::sui_unit(), false, test.ctx().epoch()).to_order();
        let has_next = order_info.match_maker(&mut maker_order, 0);
        assert!(has_next, 0);
        assert!(order_info.fills().length() == 1, 0);
        assert!(order_info.executed_quantity() == 100 * constants::sui_unit(), 0);
        // 100 * 1813.05 = 181305 = 181305000000
        assert!(order_info.cumulative_quote_quantity() == 181_305_000_000, 0);
        assert!(order_info.status() == constants::partially_filled(), 0);
        assert!(order_info.remaining_quantity() == 31_110_000_000, 0);
        let (settled, owed) = order_info.calculate_partial_fill_balances(constants::taker_fee(), constants::maker_fee());

        // 100 SUI filled, the taker is owed 100 SUI.
        assert_eq(settled, balances::new(100_000_000_000, 0, 0));
        // Taker paid 181305 USDC for 100 SUI, so they owe 181305 USDC.
        // The remaining 31.11 SUI is placed as a maker order at $1900
        // Additional owed to create maker order 31.11 * 1900 = 59109 USDC.
        // Total USDC owed = 181305 + 59109 = 240414

        // Taker fee = 0.001 * 100 = 0.1 DEEP
        // Maker fee = 0.0005 * 31.11 = 0.015555
        // Total fees owed = 0.1 + 0.015555 = 0.115555 = 115555000
        assert_eq(owed, balances::new(0, 240_414_000_000, 115_555_000));

        end(test);
    }

    #[test]
    // Place an ask order with quantity 0.005 at price $68,191.55. Partial fill of 0.001 at $70,000
    fun calculate_partial_fill_balances_ask_partial_fill_ok() {
        let mut test = begin(OWNER);

        test.next_tx(ALICE);
        let price = 68_191_550_000;
        let maker_price = 70_000_000_000;
        let quantity = 5_000_000;
        let mut order_info = create_order_info_base(ALICE, price, quantity, false, test.ctx().epoch());
        let mut maker_order = create_order_info_base(BOB, maker_price, 1_000_000, true, test.ctx().epoch()).to_order();
        let has_next = order_info.match_maker(&mut maker_order, 0);
        assert!(has_next, 0);
        assert!(order_info.fills().length() == 1, 0);
        assert!(order_info.executed_quantity() == 1_000_000, 0);
        // 0.001 * 70,000 = 70 = 70000000
        assert!(order_info.cumulative_quote_quantity() == 70_000_000, 0);
        assert!(order_info.status() == constants::partially_filled(), 0);
        assert!(order_info.remaining_quantity() == 4_000_000, 0);
        let (settled, owed) = order_info.calculate_partial_fill_balances(constants::taker_fee(), constants::maker_fee());

        // Sell of 0.001 SUI filled at $70,000, taker is owed 70 USDC
        assert_eq(settled, balances::new(0, 70_000_000, 0));
        // Taker paid 70 USDC for 0.001 SUI, so they owe 70 USDC.
        // The remaining 0.004 SUI is placed as a maker order at $68,191.55

        // Taker fee = 0.001 * 0.001 = 0.000001 DEEP
        // Maker fee = 0.0005 * 0.004 = 0.000002 DEEP
        // Total fees owed = 0.000003 DEEP = 3000
        assert_eq(owed, balances::new(5_000_000, 0, 3_000));

        end(test);
    }

    #[test]
    // Place a bid order with quantity 999.99 at price $111.11. Full fill.
    fun calculate_partial_fill_balances_bid_full_fill_ok() {
        let mut test = begin(OWNER);

        test.next_tx(ALICE);
        let price = 111_110_000;
        let maker_price = 111_110_000;
        let quantity = 999_990_000_000;
        let mut order_info = create_order_info_base(ALICE, price, quantity, true, test.ctx().epoch());
        let mut maker_order = create_order_info_base(BOB, maker_price, 999_990_000_000, false, test.ctx().epoch()).to_order();
        let has_next = order_info.match_maker(&mut maker_order, 0);
        assert!(has_next, 0);
        assert!(order_info.fills().length() == 1, 0);
        assert!(order_info.executed_quantity() == 999_990_000_000, 0);
        // 999.99 * 111.11 = 111108.8889 = 111108888900
        assert!(order_info.cumulative_quote_quantity() == 111_108_888_900, 0);
        assert!(order_info.status() == constants::filled(), 0);
        assert!(order_info.remaining_quantity() == 0, 0);

        end(test);
    }

    #[test]
    // Place an ask order with quantity 0.0001 at price $1,000,000. Full fill.
    fun calculate_partial_fill_balances_ask_full_fill_ok() {
        let mut test = begin(OWNER);

        test.next_tx(ALICE);
        let price = 1_000_000_000_000;
        let maker_price = 1_000_000_000_000;
        let quantity = 100_000;
        let mut order_info = create_order_info_base(ALICE, price, quantity, false, test.ctx().epoch());
        let mut maker_order = create_order_info_base(BOB, maker_price, 100_000, true, test.ctx().epoch()).to_order();
        let has_next = order_info.match_maker(&mut maker_order, 0);
        assert!(has_next, 0);
        assert!(order_info.fills().length() == 1, 0);
        assert!(order_info.executed_quantity() == 100_000, 0);
        // 0.0001 * 1,000,000 = 100 = 100000000
        assert!(order_info.cumulative_quote_quantity() == 100_000_000, 0);
        assert!(order_info.status() == constants::filled(), 0);
        assert!(order_info.remaining_quantity() == 0, 0);

        end(test);
    }


    #[test, expected_failure(abort_code = order_info::EOrderBelowMinimumSize)]
    fun validate_inputs_below_minimum_e() {
        let mut test = begin(OWNER);

        test.next_tx(ALICE);
        let price = 1_000_000;
        let quantity = 100;
        create_order_info_base(ALICE, price, quantity, true, test.ctx().epoch());

        abort(0)
    }

    #[test, expected_failure(abort_code = order_info::EOrderInvalidLotSize)]
    fun validate_inputs_invalid_lot_size_e() {
        let mut test = begin(OWNER);

        test.next_tx(ALICE);
        let price = 1_000_000;
        let quantity = 100_100_100;
        create_order_info_base(ALICE, price, quantity, true, test.ctx().epoch());

        abort(0)
    }

    #[test, expected_failure(abort_code = order_info::EInvalidOrderType)]
    fun validate_inputs_invalid_order_type_e() {
        let mut test = begin(OWNER);

        test.next_tx(ALICE);
        let price = 1_000_000;
        let quantity = 100_000;
        let balance_manager_id = id_from_address(@0x1);
        let order_type = 5;
        let fee_is_deep = true;
        let deep_per_asset = 1 * constants::float_scaling();
        let market_order = false;
        let expire_timestamp = constants::max_u64();
        let conversion_is_base = true;
        create_order_info(
            balance_manager_id,
            ALICE,
            order_type,
            price,
            quantity,
            true,
            fee_is_deep,
            test.ctx().epoch(),
            expire_timestamp,
            deep_per_asset,
            conversion_is_base,
            market_order
        );

        abort(0)
    }

    #[test, expected_failure(abort_code = order_info::EMarketOrderCannotBePostOnly)]
    fun validate_inputs_market_order_post_only_e() {
        let mut test = begin(OWNER);

        test.next_tx(ALICE);
        let price = 1_000_000;
        let quantity = 100_000;
        let balance_manager_id = id_from_address(@0x1);
        let order_type = 3;
        let fee_is_deep = true;
        let deep_per_asset = 1 * constants::float_scaling();
        let market_order = true;
        let expire_timestamp = constants::max_u64();
        let conversion_is_base = true;
        create_order_info(
            balance_manager_id,
            ALICE,
            order_type,
            price,
            quantity,
            true,
            fee_is_deep,
            test.ctx().epoch(),
            expire_timestamp,
            deep_per_asset,
            conversion_is_base,
            market_order
        );

        abort(0)
    }

    #[test, expected_failure(abort_code = order_info::EOrderInvalidPrice)]
    fun validate_inputs_invalid_price_e() {
        let mut test = begin(OWNER);

        test.next_tx(ALICE);
        let price = 0;
        let quantity = 100_000;
        create_order_info_base(ALICE, price, quantity, true, test.ctx().epoch());

        abort(0)
    }

    #[test, expected_failure(abort_code = order_info::EOrderInvalidPrice)]
    fun validate_inputs_invalid_price2_e() {
        let mut test = begin(OWNER);

        test.next_tx(ALICE);
        let price = 111;
        let quantity = 100_000;
        create_order_info_base(ALICE, price, quantity, true, test.ctx().epoch());

        abort(0)
    }

    #[test, expected_failure(abort_code = order_info::EPOSTOrderCrossesOrderbook)]
    fun validate_execution_post_only_e() {
        let mut test = begin(OWNER);

        test.next_tx(ALICE);
        let price = 1_000_000;
        let quantity = 100_000;
        let balance_manager_id = id_from_address(@0x1);
        let order_type = 3;
        let fee_is_deep = true;
        let deep_per_asset = 1 * constants::float_scaling();
        let market_order = false;
        let expire_timestamp = constants::max_u64();
        let conversion_is_base = true;
        let mut order_info = create_order_info(
            balance_manager_id,
            ALICE,
            order_type,
            price,
            quantity,
            true,
            fee_is_deep,
            test.ctx().epoch(),
            expire_timestamp,
            deep_per_asset,
            conversion_is_base,
            market_order
        );
        let mut maker_order = create_order_info_base(BOB, price, 1_000_000, false, test.ctx().epoch()).to_order();
        order_info.match_maker(&mut maker_order, 0);
        order_info.assert_execution();

        abort(0)
    }

    #[test, expected_failure(abort_code = order_info::EFOKOrderCannotBeFullyFilled)]
    fun validate_execution_FOK_e() {
        let mut test = begin(OWNER);

        test.next_tx(ALICE);
        let price = 1_000_000;
        let quantity = 100_000_000;
        let balance_manager_id = id_from_address(@0x1);
        let order_type = 2;
        let fee_is_deep = true;
        let deep_per_asset = 1 * constants::float_scaling();
        let market_order = false;
        let expire_timestamp = constants::max_u64();
        let conversion_is_base = true;
        let mut order_info = create_order_info(
            balance_manager_id,
            ALICE,
            order_type,
            price,
            quantity,
            true,
            fee_is_deep,
            test.ctx().epoch(),
            expire_timestamp,
            deep_per_asset,
            conversion_is_base,
            market_order
        );
        let mut maker_order = create_order_info_base(BOB, price, 1_000_000, true, test.ctx().epoch()).to_order();
        order_info.match_maker(&mut maker_order, 0);
        order_info.assert_execution();

        abort(0)
    }

    #[test]
    fun validate_execution_immediate_or_cancel_ok() {
        let mut test = begin(OWNER);

        test.next_tx(ALICE);
        let price = 1_000_000;
        let quantity = 100_000_000;
        let balance_manager_id = id_from_address(@0x1);
        let order_type = 1;
        let fee_is_deep = true;
        let deep_per_asset = 1 * constants::float_scaling();
        let market_order = false;
        let expire_timestamp = constants::max_u64();
        let conversion_is_base = true;
        let mut order_info = create_order_info(
            balance_manager_id,
            ALICE,
            order_type,
            price,
            quantity,
            true,
            fee_is_deep,
            test.ctx().epoch(),
            expire_timestamp,
            deep_per_asset,
            conversion_is_base,
            market_order
        );
        let mut maker_order = create_order_info_base(BOB, price, 1_000_000, true, test.ctx().epoch()).to_order();
        order_info.match_maker(&mut maker_order, 0);
        order_info.assert_execution();
        assert!(order_info.status() == constants::canceled(), 0);

        end(test);
    }

    #[test_only]
    public fun create_order_info_base(
        trader: address,
        price: u64,
        quantity: u64,
        is_bid: bool,
        epoch: u64,
    ): OrderInfo {
        let balance_manager_id = id_from_address(trader);
        let order_type = 0;
        let fee_is_deep = true;
        let deep_per_asset = 1 * constants::float_scaling();
        let market_order = false;
        let expire_timestamp = constants::max_u64();
        let conversion_is_deep = true;

        create_order_info(
            balance_manager_id,
            trader,
            order_type,
            price,
            quantity,
            is_bid,
            fee_is_deep,
            epoch,
            expire_timestamp,
            deep_per_asset,
            conversion_is_deep,
            market_order
        )
    }

    #[test_only]
    public fun create_order_info(
        balance_manager_id: ID,
        trader: address,
        order_type: u8,
        price: u64,
        quantity: u64,
        is_bid: bool,
        fee_is_deep: bool,
        epoch: u64,
        expire_timestamp: u64,
        deep_per_asset: u64,
        conversion_is_base: bool,
        market_order: bool,
    ): OrderInfo {
        let pool_id = id_from_address(@0x2);
        let client_order_id = 1;
        let order_deep_price = deep_price::new_order_deep_price(conversion_is_base, deep_per_asset);
        let mut order_info = order_info::new(
            pool_id,
            balance_manager_id,
            client_order_id,
            trader,
            order_type,
            constants::self_matching_allowed(),
            price,
            quantity,
            is_bid,
            fee_is_deep,
            epoch,
            expire_timestamp,
            order_deep_price,
            market_order,
        );

        order_info.set_order_id(utils::encode_order_id(is_bid, price, 1));
        order_info.validate_inputs(
            constants::tick_size(),
            constants::min_size(),
            constants::lot_size(),
            0
        );

        order_info
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module deepbook::pool_tests {
    use sui::{
        clock::{Self, Clock},
        test_scenario::{
            Scenario,
            begin,
            end,
            return_shared,
        },
        coin::{Self, Coin},
        sui::SUI,
        coin::mint_for_testing,
        test_utils,
    };

    use deepbook::{
        pool::{Self, Pool},
        balance_manager::{BalanceManager, TradeCap},
        order::{Order},
        order_info::OrderInfo,
        big_vector::BigVector,
        math,
        registry::{Self, Registry},
        constants,
        utils,
        balance_manager_tests::{USDC, SPAM, create_acct_and_share_with_funds as create_acct_and_share_with_funds},
    };

    use token::deep::DEEP;

    const OWNER: address = @0x1;
    const ALICE: address = @0xAAAA;
    const BOB: address = @0xBBBB;

    #[test]
    fun test_place_order_bid() {
        place_order_ok(true);
    }

    #[test]
    fun test_place_order_ask() {
        place_order_ok(false);
    }

    #[test]
    fun test_place_and_cancel_order_bid() {
        place_and_cancel_order_ok(true);
    }

    #[test]
    fun test_place_and_cancel_order_ask() {
        place_and_cancel_order_ok(false);
    }

    #[test]
    fun test_place_then_fill_bid_ask() {
        place_then_fill(
            false,
            true,
            constants::no_restriction(),
            3 * constants::float_scaling(),
            3 * constants::float_scaling(),
            6 * constants::float_scaling(),
            3 * math::mul(constants::taker_fee(), constants::deep_multiplier()),
            constants::filled()
        );
    }

    #[test]
    fun test_place_then_fill_bid_ask_stable() {
        place_then_fill(
            true,
            true,
            constants::no_restriction(),
            3 * constants::float_scaling(),
            3 * constants::float_scaling(),
            6 * constants::float_scaling(),
            3 * math::mul(constants::stable_taker_fee(), constants::deep_multiplier()),
            constants::filled()
        );
    }

    #[test]
    fun test_place_then_fill_ask_bid() {
        place_then_fill(
            false,
            false,
            constants::no_restriction(),
            3 * constants::float_scaling(),
            3 * constants::float_scaling(),
            6 * constants::float_scaling(),
            3 * math::mul(constants::taker_fee(), constants::deep_multiplier()),
            constants::filled()
        );
    }

    #[test]
    fun test_place_then_fill_ask_bid_stable() {
        place_then_fill(
            true,
            false,
            constants::no_restriction(),
            3 * constants::float_scaling(),
            3 * constants::float_scaling(),
            6 * constants::float_scaling(),
            3 * math::mul(constants::stable_taker_fee(), constants::deep_multiplier()),
            constants::filled()
        );
    }

    #[test]
    fun test_place_then_ioc_bid_ask() {
        place_then_fill(
            false,
            true,
            constants::immediate_or_cancel(),
            3 * constants::float_scaling(),
            3 * constants::float_scaling(),
            6 * constants::float_scaling(),
            3 * math::mul(constants::taker_fee(), constants::deep_multiplier()),
            constants::filled()
        );
    }

    #[test]
    fun test_place_then_ioc_bid_ask_stable() {
        place_then_fill(
            true,
            true,
            constants::immediate_or_cancel(),
            3 * constants::float_scaling(),
            3 * constants::float_scaling(),
            6 * constants::float_scaling(),
            3 * math::mul(constants::stable_taker_fee(), constants::deep_multiplier()),
            constants::filled()
        );
    }

    #[test]
    fun test_place_then_ioc_ask_bid() {
        place_then_fill(
            false,
            false,
            constants::immediate_or_cancel(),
            3 * constants::float_scaling(),
            3 * constants::float_scaling(),
            6 * constants::float_scaling(),
            3 * math::mul(constants::taker_fee(), constants::deep_multiplier()),
            constants::filled()
        );
    }

    #[test]
    fun test_place_then_ioc_ask_bid_stable() {
        place_then_fill(
            true,
            false,
            constants::immediate_or_cancel(),
            3 * constants::float_scaling(),
            3 * constants::float_scaling(),
            6 * constants::float_scaling(),
            3 * math::mul(constants::stable_taker_fee(), constants::deep_multiplier()),
            constants::filled()
        );
    }

    #[test, expected_failure(abort_code = ::deepbook::big_vector::ENotFound)]
    fun test_place_then_ioc_no_fill_bid_ask_order_removed_e() {
        place_then_no_fill(
            true,
            constants::immediate_or_cancel(),
            0,
            0,
            0,
            constants::canceled()
        );
    }

    #[test, expected_failure(abort_code = ::deepbook::big_vector::ENotFound)]
    fun test_place_then_ioc_no_fill_ask_bid_order_removed_e() {
        place_then_no_fill(
            false,
            constants::immediate_or_cancel(),
            0,
            0,
            0,
            constants::canceled()
        );
    }

    #[test, expected_failure(abort_code = ::deepbook::big_vector::ENotFound)]
    fun test_expired_order_removed_bid_ask_e(){
        place_order_expire_timestamp_e(
            true,
            constants::no_restriction(),
            0,
            0,
            0,
            constants::live(),
        );
    }

    #[test, expected_failure(abort_code = ::deepbook::big_vector::ENotFound)]
    fun test_expired_order_removed_ask_bid_e(){
        place_order_expire_timestamp_e(
            false,
            constants::no_restriction(),
            0,
            0,
            0,
            constants::live(),
        );
    }

    #[test]
    fun test_partial_fill_order_bid() {
        partial_fill_order(
            true,
            constants::no_restriction(),
            3 * constants::float_scaling(),
            3 * constants::float_scaling(),
            6 * constants::float_scaling(),
            3 * math::mul(constants::taker_fee(), constants::deep_multiplier()),
            constants::partially_filled()
        );
    }

    #[test]
    fun test_partial_fill_order_ask() {
        partial_fill_order(
            false,
            constants::no_restriction(),
            3 * constants::float_scaling(),
            3 * constants::float_scaling(),
            6 * constants::float_scaling(),
            3 * math::mul(constants::taker_fee(), constants::deep_multiplier()),
            constants::partially_filled()
        );
    }

    #[test, expected_failure(abort_code = ::deepbook::order_info::EOrderBelowMinimumSize)]
    fun test_invalid_order_quantity_e() {
        place_with_price_quantity(
            2 * constants::float_scaling(),
            0
        );
    }

    #[test, expected_failure(abort_code = ::deepbook::order_info::EOrderInvalidLotSize)]
    fun test_invalid_lot_size_e() {
        place_with_price_quantity(
            2 * constants::float_scaling(),
            1_000_000_100,
        );
    }

    #[test, expected_failure(abort_code = ::deepbook::order_info::EOrderInvalidPrice)]
    fun test_invalid_tick_size_e() {
        place_with_price_quantity(
            2_000_000_100,
            1 * constants::float_scaling(),
        );
    }

    #[test, expected_failure(abort_code = ::deepbook::order_info::EOrderInvalidPrice)]
    fun test_price_above_max_e() {
        place_with_price_quantity(
            constants::max_u64(),
            1 * constants::float_scaling(),
        );
    }

    #[test, expected_failure(abort_code = ::deepbook::order_info::EOrderInvalidPrice)]
    fun test_price_below_min_e() {
        place_with_price_quantity(
            0,
            1 * constants::float_scaling(),
        );
    }

    #[test, expected_failure(abort_code = ::deepbook::order_info::ESelfMatchingCancelTaker)]
    fun test_self_matching_cancel_taker_bid() {
        test_self_matching_cancel_taker(true);
    }

    #[test, expected_failure(abort_code = ::deepbook::order_info::ESelfMatchingCancelTaker)]
    fun test_self_matching_cancel_taker_ask() {
        test_self_matching_cancel_taker(false);
    }

    #[test, expected_failure(abort_code = ::deepbook::big_vector::ENotFound)]
    fun test_self_matching_cancel_maker_bid() {
        test_self_matching_cancel_maker(true);
    }

    #[test, expected_failure(abort_code = ::deepbook::big_vector::ENotFound)]
    fun test_self_matching_cancel_maker_ask() {
        test_self_matching_cancel_maker(false);
    }

    #[test]
    fun test_swap_exact_amount_bid_ask() {
        test_swap_exact_amount(true);
    }

    #[test]
    fun test_swap_exact_amount_ask_bid() {
        test_swap_exact_amount(false);
    }

    #[test, expected_failure(abort_code = ::deepbook::big_vector::ENotFound)]
    fun test_cancel_all_orders_bid() {
        test_cancel_all_orders(true);
    }

    #[test, expected_failure(abort_code = ::deepbook::big_vector::ENotFound)]
    fun test_cancel_all_orders_ask() {
        test_cancel_all_orders(false);
    }

    #[test, expected_failure(abort_code = ::deepbook::order_info::EPOSTOrderCrossesOrderbook)]
    fun test_post_only_bid_e() {
        test_post_only(true, true);
    }

    #[test, expected_failure(abort_code = ::deepbook::order_info::EPOSTOrderCrossesOrderbook)]
    fun test_post_only_ask_e() {
        test_post_only(false, true);
    }

    #[test]
    fun test_post_only_bid_ok() {
        test_post_only(true, false);
    }

    #[test]
    fun test_post_only_ask_ok() {
        test_post_only(false, false);
    }

    #[test]
    fun test_crossing_multiple_orders_bid_ok() {
        test_crossing_multiple(true, 3)
    }

    #[test]
    fun test_crossing_multiple_orders_ask_ok() {
        test_crossing_multiple(false, 3)
    }

    #[test, expected_failure(abort_code = ::deepbook::order_info::EFOKOrderCannotBeFullyFilled)]
    fun test_fill_or_kill_bid_e() {
        test_fill_or_kill(true, false);
    }

    #[test, expected_failure(abort_code = ::deepbook::order_info::EFOKOrderCannotBeFullyFilled)]
    fun test_fill_or_kill_ask_e() {
        test_fill_or_kill(false, false);
    }

    #[test]
    fun test_fill_or_kill_bid_ok() {
        test_fill_or_kill(true, true);
    }

    #[test]
    fun test_fill_or_kill_ask_ok() {
        test_fill_or_kill(false, true);
    }

    #[test]
    fun test_market_order_bid_then_ask_ok() {
        test_market_order(true);
    }

    #[test]
    fun test_market_order_ask_then_bid_ok() {
        test_market_order(false);
    }

    #[test]
    fun test_mid_price_ok() {
        test_mid_price();
    }

    #[test]
    fun test_swap_exact_not_fully_filled_bid_ok(){
        test_swap_exact_not_fully_filled(true);
    }

    #[test]
    fun test_swap_exact_not_fully_filled_ask_ok(){
        test_swap_exact_not_fully_filled(false);
    }

    #[test]
    fun test_unregister_pool_ok(){
        test_unregister_pool(true);
    }

    #[test, expected_failure(abort_code = ::deepbook::registry::EPoolAlreadyExists)]
    fun test_duplicate_pool_e(){
        test_unregister_pool(false);
    }

    #[test]
    fun test_get_pool_id_by_asset_ok(){
        test_get_pool_id_by_asset();
    }

    #[test]
    fun test_modify_order_bid_ok(){
        test_modify_order(
            3 * constants::float_scaling(),
            2 * constants::float_scaling(),
            0,
            true,
        );
    }

    #[test]
    fun test_modify_order_ask_ok(){
        test_modify_order(
            3 * constants::float_scaling(),
            2 * constants::float_scaling(),
            0,
            false,
        );
    }

    #[test, expected_failure(abort_code = ::deepbook::book::ENewQuantityMustBeLessThanOriginal)]
    fun test_modify_order_increase_bid_e(){
        test_modify_order(
            2 * constants::float_scaling(),
            3 * constants::float_scaling(),
            0,
            true,
        );
    }

    #[test, expected_failure(abort_code = ::deepbook::book::ENewQuantityMustBeLessThanOriginal)]
    fun test_modify_order_increase_ask_e(){
        test_modify_order(
            2 * constants::float_scaling(),
            3 * constants::float_scaling(),
            0,
            false,
        );
    }

    #[test, expected_failure(abort_code = ::deepbook::order::EInvalidNewQuantity)]
    fun test_modify_order_invalid_new_quantity_bid_e(){
        test_modify_order(
            3 * constants::float_scaling(),
            2 * constants::float_scaling(),
            2 * constants::float_scaling(),
            true,
        );
    }

    #[test, expected_failure(abort_code = ::deepbook::order::EInvalidNewQuantity)]
    fun test_modify_order_invalid_new_quantity_ask_e(){
        test_modify_order(
            3 * constants::float_scaling(),
            2 * constants::float_scaling(),
            2 * constants::float_scaling(),
            false,
        );
    }

    #[test]
    fun test_queue_priority_bid_ok(){
        test_queue_priority(true);
    }

    #[test]
    fun test_queue_priority_ask_ok(){
        test_queue_priority(false);
    }

    #[test_only]
    public(package) fun setup_test(
        owner: address,
        test: &mut Scenario,
    ): ID {
        test.next_tx(owner);
        share_clock(test);
        share_registry_for_testing(test)
    }

    #[test_only]
    public(package) fun add_deep_price_point<BaseAsset, QuoteAsset, ReferenceBaseAsset, ReferenceQuoteAsset>(
        sender: address,
        target_pool_id: ID,
        reference_pool_id: ID,
        test: &mut Scenario,
    ){
        test.next_tx(sender);
        {
            let mut target_pool = test.take_shared_by_id<Pool<BaseAsset, QuoteAsset>>(target_pool_id);
            let reference_pool = test.take_shared_by_id<Pool<ReferenceBaseAsset, ReferenceQuoteAsset>>(reference_pool_id);
            let clock = test.take_shared<Clock>();
            pool::add_deep_price_point<BaseAsset, QuoteAsset, ReferenceBaseAsset, ReferenceQuoteAsset>(
                &mut target_pool,
                &reference_pool,
                &clock
            );
            return_shared(target_pool);
            return_shared(reference_pool);
            return_shared(clock);
        }
    }

    #[test_only]
    /// Set up a reference pool where Deep per base is 100
    public(package) fun setup_reference_pool<BaseAsset, QuoteAsset>(
        sender: address,
        registry_id: ID,
        balance_manager_id: ID,
        mid_price: u64,
        test: &mut Scenario,
    ): ID {
        let reference_pool_id = setup_pool_with_default_fees<BaseAsset, QuoteAsset>(
            sender,
            registry_id,
            true,
            test,
        );

        place_limit_order<BaseAsset, QuoteAsset>(
            sender,
            reference_pool_id,
            balance_manager_id,
            1,
            constants::no_restriction(),
            constants::self_matching_allowed(),
            mid_price - 80 * constants::float_scaling(),
            1 * constants::float_scaling(),
            true,
            true,
            constants::max_u64(),
            test,
        );

        place_limit_order<BaseAsset, QuoteAsset>(
            sender,
            reference_pool_id,
            balance_manager_id,
            1,
            constants::no_restriction(),
            constants::self_matching_allowed(),
            mid_price + 80 * constants::float_scaling(),
            1 * constants::float_scaling(),
            false,
            true,
            constants::max_u64(),
            test,
        );

        reference_pool_id
    }

    #[test_only]
    public(package) fun setup_pool_with_default_fees<BaseAsset, QuoteAsset>(
        sender: address,
        registry_id: ID,
        whitelisted_pool: bool,
        test: &mut Scenario,
    ): ID {
        let creation_fee = coin::mint_for_testing<DEEP>(constants::pool_creation_fee(), test.ctx());
        let stable_pool = false;
        setup_pool<BaseAsset, QuoteAsset>(
            sender,
            constants::tick_size(), // tick size
            constants::lot_size(), // lot size
            constants::min_size(), // min size
            registry_id,
            whitelisted_pool,
            stable_pool,
            creation_fee,
            test,
        )
    }

    #[test_only]
    public(package) fun setup_pool_with_stable_fees<BaseAsset, QuoteAsset>(
        sender: address,
        registry_id: ID,
        whitelisted_pool: bool,
        test: &mut Scenario,
    ): ID {
        let creation_fee = coin::mint_for_testing<DEEP>(constants::pool_creation_fee(), test.ctx());
        let stable_pool = true;
        setup_pool<BaseAsset, QuoteAsset>(
            sender,
            constants::tick_size(), // tick size
            constants::lot_size(), // lot size
            constants::min_size(), // min size
            registry_id,
            whitelisted_pool,
            stable_pool,
            creation_fee,
            test,
        )
    }

    #[test_only]
    public(package) fun setup_pool_with_default_fees_return_fee<BaseAsset, QuoteAsset>(
        sender: address,
        registry_id: ID,
        whitelisted_pool: bool,
        test: &mut Scenario,
    ): (ID, ID) {
        let creation_fee = coin::mint_for_testing<DEEP>(constants::pool_creation_fee(), test.ctx());
        let fee_id = object::id(&creation_fee);
        let stable_pool = false;
        let pool_id = setup_pool<BaseAsset, QuoteAsset>(
            sender,
            constants::tick_size(), // tick size
            constants::lot_size(), // lot size
            constants::min_size(), // min size
            registry_id,
            whitelisted_pool,
            stable_pool,
            creation_fee,
            test,
        );

        (pool_id, fee_id)
    }

    #[test_only]
    /// Place a limit order
    public(package) fun place_limit_order<BaseAsset, QuoteAsset>(
        trader: address,
        pool_id: ID,
        balance_manager_id: ID,
        client_order_id: u64,
        order_type: u8,
        self_matching_option: u8,
        price: u64,
        quantity: u64,
        is_bid: bool,
        pay_with_deep: bool,
        expire_timestamp: u64,
        test: &mut Scenario,
    ): OrderInfo {
        test.next_tx(trader);
        {
            let mut pool = test.take_shared_by_id<Pool<BaseAsset, QuoteAsset>>(pool_id);
            let clock = test.take_shared<Clock>();
            let mut balance_manager = test.take_shared_by_id<BalanceManager>(balance_manager_id);
            let trade_cap = test.take_from_sender<TradeCap>();
            let trade_proof = balance_manager.generate_proof_as_trader(&trade_cap, test.ctx());
            // let trade_proof = balance_manager.generate_proof_as_owner(test.ctx());

            // Place order in pool
            let order_info = pool.place_limit_order<BaseAsset, QuoteAsset>(
                &mut balance_manager,
                &trade_proof,
                client_order_id,
                order_type,
                self_matching_option,
                price,
                quantity,
                is_bid,
                pay_with_deep,
                expire_timestamp,
                &clock,
                test.ctx()
            );
            test.return_to_sender(trade_cap);
            return_shared(pool);
            return_shared(clock);
            return_shared(balance_manager);

            order_info
        }
    }

    #[test_only]
    /// Place an order
    public(package) fun place_market_order<BaseAsset, QuoteAsset>(
        trader: address,
        pool_id: ID,
        balance_manager_id: ID,
        client_order_id: u64,
        self_matching_option: u8,
        quantity: u64,
        is_bid: bool,
        pay_with_deep: bool,
        test: &mut Scenario,
    ): OrderInfo {
        test.next_tx(trader);
        {
            let mut pool = test.take_shared_by_id<Pool<BaseAsset, QuoteAsset>>(pool_id);
            let clock = test.take_shared<Clock>();
            let mut balance_manager = test.take_shared_by_id<BalanceManager>(balance_manager_id);
            let trade_proof = balance_manager.generate_proof_as_owner(test.ctx());

            // Place order in pool
            let order_info = pool.place_market_order<BaseAsset, QuoteAsset>(
                &mut balance_manager,
                &trade_proof,
                client_order_id,
                self_matching_option,
                quantity,
                is_bid,
                pay_with_deep,
                &clock,
                test.ctx()
            );
            return_shared(pool);
            return_shared(clock);
            return_shared(balance_manager);

            order_info
        }
    }

    #[test_only]
    /// Cancel an order
    public(package) fun cancel_order<BaseAsset, QuoteAsset>(
        sender: address,
        pool_id: ID,
        balance_manager_id: ID,
        order_id: u128,
        test: &mut Scenario,
    ) {
        test.next_tx(sender);
        {
            let mut pool = test.take_shared_by_id<Pool<BaseAsset, QuoteAsset>>(pool_id);
            let clock = test.take_shared<Clock>();
            let mut balance_manager = test.take_shared_by_id<BalanceManager>(balance_manager_id);
            let trade_proof = balance_manager.generate_proof_as_owner(test.ctx());

            pool.cancel_order<BaseAsset, QuoteAsset>(
                &mut balance_manager,
                &trade_proof,
                order_id,
                &clock,
                test.ctx()
            );
            return_shared(pool);
            return_shared(clock);
            return_shared(balance_manager);
        }
    }

    #[test_only]
    /// Set the time in the global clock to 1_000_000 + current_time
    public(package) fun set_time(
        current_time: u64,
        test: &mut Scenario,
    ) {
        test.next_tx(OWNER);
        {
            let mut clock = test.take_shared<Clock>();
            clock.set_for_testing(current_time + 1_000_000);
            return_shared(clock);
        };
    }

    #[test_only]
    public(package) fun modify_order<BaseAsset, QuoteAsset>(
        sender: address,
        pool_id: ID,
        balance_manager_id: ID,
        order_id: u128,
        new_quantity: u64,
        test: &mut Scenario,
    ) {
        test.next_tx(sender);
        {
            let mut pool = test.take_shared_by_id<Pool<BaseAsset, QuoteAsset>>(pool_id);
            let mut balance_manager = test.take_shared_by_id<BalanceManager>(balance_manager_id);
            let trade_cap = test.take_from_sender<TradeCap>();
            let trade_proof = balance_manager.generate_proof_as_trader(&trade_cap, test.ctx());
            let clock = test.take_shared<Clock>();

            pool.modify_order<BaseAsset, QuoteAsset>(
                &mut balance_manager,
                &trade_proof,
                order_id,
                new_quantity,
                &clock,
                test.ctx()
            );

            test.return_to_sender(trade_cap);
            return_shared(pool);
            return_shared(balance_manager);
            return_shared(clock);
        }
    }


    /// Alice places a worse order
    /// Alice places 3 bid/ask orders with at price 1
    /// Alice matches the order with an ask/bid order at price 1
    /// The first order should be matched because of queue priority
    /// Process is repeated with a third order
    fun test_queue_priority(
        is_bid: bool,
    ){
        let mut test = begin(OWNER);
        let registry_id = setup_test(OWNER, &mut test);
        let balance_manager_id_alice = create_acct_and_share_with_funds(ALICE, 1000000 * constants::float_scaling(), &mut test);
        let pool_id = setup_pool_with_default_fees_and_reference_pool<SUI, USDC, SUI, DEEP>(ALICE, registry_id, balance_manager_id_alice, &mut test);

        let client_order_id = 1;
        let price = 2 * constants::float_scaling();
        let worse_price = if (is_bid) {
            1 * constants::float_scaling()
        } else {
            3 * constants::float_scaling()
        };
        let quantity = 1 * constants::float_scaling();
        let expire_timestamp = constants::max_u64();
        let pay_with_deep = true;

        let order_info_worse = place_limit_order<SUI, USDC>(
            ALICE,
            pool_id,
            balance_manager_id_alice,
            client_order_id,
            constants::no_restriction(),
            constants::self_matching_allowed(),
            worse_price,
            quantity,
            is_bid,
            pay_with_deep,
            expire_timestamp,
            &mut test,
        );

        place_limit_order<SUI, USDC>(
            ALICE,
            pool_id,
            balance_manager_id_alice,
            client_order_id,
            constants::no_restriction(),
            constants::self_matching_allowed(),
            price,
            quantity,
            is_bid,
            pay_with_deep,
            expire_timestamp,
            &mut test,
        );

        let order_info_2 = place_limit_order<SUI, USDC>(
            ALICE,
            pool_id,
            balance_manager_id_alice,
            client_order_id,
            constants::no_restriction(),
            constants::self_matching_allowed(),
            price,
            quantity,
            is_bid,
            pay_with_deep,
            expire_timestamp,
            &mut test,
        );

        let order_info_3 = place_limit_order<SUI, USDC>(
            ALICE,
            pool_id,
            balance_manager_id_alice,
            client_order_id,
            constants::no_restriction(),
            constants::self_matching_allowed(),
            price,
            quantity,
            is_bid,
            pay_with_deep,
            expire_timestamp,
            &mut test,
        );

        // Alice places limit order at price 1 for matching
        place_limit_order<SUI, USDC>(
            ALICE,
            pool_id,
            balance_manager_id_alice,
            client_order_id,
            constants::no_restriction(),
            constants::self_matching_allowed(),
            price,
            quantity,
            !is_bid,
            pay_with_deep,
            expire_timestamp,
            &mut test,
        );

        borrow_and_verify_book_order<SUI, USDC>(
            pool_id,
            order_info_2.order_id(),
            is_bid,
            client_order_id,
            quantity,
            0,
            order_info_2.order_deep_price().deep_per_asset(),
            test.ctx().epoch(),
            constants::live(),
            expire_timestamp,
            &mut test,
        );

        borrow_and_verify_book_order<SUI, USDC>(
            pool_id,
            order_info_3.order_id(),
            is_bid,
            client_order_id,
            quantity,
            0,
            order_info_3.order_deep_price().deep_per_asset(),
            test.ctx().epoch(),
            constants::live(),
            expire_timestamp,
            &mut test,
        );

        borrow_and_verify_book_order<SUI, USDC>(
            pool_id,
            order_info_worse.order_id(),
            is_bid,
            client_order_id,
            quantity,
            0,
            order_info_3.order_deep_price().deep_per_asset(),
            test.ctx().epoch(),
            constants::live(),
            expire_timestamp,
            &mut test,
        );

        // Alice places limit order at price 1 for matching
        place_limit_order<SUI, USDC>(
            ALICE,
            pool_id,
            balance_manager_id_alice,
            client_order_id,
            constants::no_restriction(),
            constants::self_matching_allowed(),
            price,
            quantity,
            !is_bid,
            pay_with_deep,
            expire_timestamp,
            &mut test,
        );

        borrow_and_verify_book_order<SUI, USDC>(
            pool_id,
            order_info_3.order_id(),
            is_bid,
            client_order_id,
            quantity,
            0,
            order_info_3.order_deep_price().deep_per_asset(),
            test.ctx().epoch(),
            constants::live(),
            expire_timestamp,
            &mut test,
        );

        borrow_and_verify_book_order<SUI, USDC>(
            pool_id,
            order_info_worse.order_id(),
            is_bid,
            client_order_id,
            quantity,
            0,
            order_info_worse.order_deep_price().deep_per_asset(),
            test.ctx().epoch(),
            constants::live(),
            expire_timestamp,
            &mut test,
        );

        end(test);
    }

    fun test_modify_order(
        original_quantity: u64,
        new_quantity: u64,
        filled_quantity: u64,
        is_bid: bool,
    ){
        let mut test = begin(OWNER);
        let registry_id = setup_test(OWNER, &mut test);
        let balance_manager_id_alice = create_acct_and_share_with_funds(ALICE, 1000000 * constants::float_scaling(), &mut test);
        let pool_id = setup_pool_with_default_fees_and_reference_pool<SUI, USDC, SUI, DEEP>(ALICE, registry_id, balance_manager_id_alice, &mut test);

        let client_order_id = 1;
        let base_price = 2 * constants::float_scaling();
        let expire_timestamp = constants::max_u64();
        let pay_with_deep = true;

        let order_info = place_limit_order<SUI, USDC>(
            ALICE,
            pool_id,
            balance_manager_id_alice,
            client_order_id,
            constants::no_restriction(),
            constants::self_matching_allowed(),
            base_price,
            original_quantity,
            is_bid,
            pay_with_deep,
            expire_timestamp,
            &mut test,
        );

        if (filled_quantity > 0) {
            place_limit_order<SUI, USDC>(
                ALICE,
                pool_id,
                balance_manager_id_alice,
                client_order_id,
                constants::no_restriction(),
                constants::self_matching_allowed(),
                base_price,
                filled_quantity,
                !is_bid,
                pay_with_deep,
                expire_timestamp,
                &mut test,
            );
        };

        modify_order<SUI, USDC>(
            ALICE,
            pool_id,
            balance_manager_id_alice,
            order_info.order_id(),
            new_quantity,
            &mut test,
        );

        borrow_and_verify_book_order<SUI, USDC>(
            pool_id,
            order_info.order_id(),
            is_bid,
            client_order_id,
            new_quantity,
            0,
            order_info.order_deep_price().deep_per_asset(),
            test.ctx().epoch(),
            constants::live(),
            expire_timestamp,
            &mut test,
        );

        end(test);
    }

    fun test_get_pool_id_by_asset(){
        let mut test = begin(OWNER);
        let registry_id = setup_test(OWNER, &mut test);
        let pool_id_1 = setup_pool_with_default_fees<SUI, USDC>(OWNER, registry_id, false, &mut test);
        let pool_id_2 = setup_pool_with_default_fees<SPAM, USDC>(OWNER, registry_id, false, &mut test);
        let pool_id_1_returned = get_pool_id_by_asset<SUI, USDC>(registry_id, &mut test);
        let pool_id_2_returned = get_pool_id_by_asset<SPAM, USDC>(registry_id, &mut test);

        assert!(pool_id_1 == pool_id_1_returned, constants::e_incorrect_pool_id());
        assert!(pool_id_2 == pool_id_2_returned, constants::e_incorrect_pool_id());
        end(test);
    }

    fun get_pool_id_by_asset<BaseAsset, QuoteAsset>(
        registry_id: ID,
        test: &mut Scenario,
    ): ID {
        test.next_tx(OWNER);
        {
            let registry = test.take_shared_by_id<Registry>(registry_id);
            let pool_id = pool::get_pool_id_by_asset<BaseAsset, QuoteAsset>(&registry);
            return_shared(registry);

            pool_id
        }
    }

    fun test_unregister_pool(
        unregister: bool,
    ){
        let mut test = begin(OWNER);
        let registry_id = setup_test(OWNER, &mut test);
        setup_pool_with_default_fees<SUI, USDC>(OWNER, registry_id, false, &mut test);
        if (unregister) {
            unregister_pool<SUI, USDC>(registry_id, &mut test);
        };
        setup_pool_with_default_fees<SUI, USDC>(OWNER, registry_id, false, &mut test);

        end(test);
    }

    fun unregister_pool<BaseAsset, QuoteAsset>(
        registry_id: ID,
        test: &mut Scenario,
    ) {
        test.next_tx(OWNER);
        {
            let admin_cap = registry::get_admin_cap_for_testing(test.ctx());
            let mut registry = test.take_shared_by_id<Registry>(registry_id);

            pool::unregister_pool_admin<BaseAsset, QuoteAsset>(&mut registry, &admin_cap);
            return_shared(registry);
            test_utils::destroy(admin_cap);
        }
    }

    fun setup_pool_with_default_fees_and_reference_pool<BaseAsset, QuoteAsset, ReferenceBaseAsset, ReferenceQuoteAsset>(
        sender: address,
        registry_id: ID,
        balance_manager_id: ID,
        test: &mut Scenario,
    ): ID {
        let target_pool_id = setup_pool_with_default_fees<BaseAsset, QuoteAsset>(
            OWNER,
            registry_id,
            false,
            test,
        );
        let reference_pool_id = setup_reference_pool<ReferenceBaseAsset, ReferenceQuoteAsset>(
            sender,
            registry_id,
            balance_manager_id,
            100 * constants::float_scaling(),
            test,
        );
        set_time(0, test);
        add_deep_price_point<BaseAsset, QuoteAsset, ReferenceBaseAsset, ReferenceQuoteAsset>(
            sender,
            target_pool_id,
            reference_pool_id,
            test,
        );

        target_pool_id
    }

    fun setup_pool_with_stable_fees_and_reference_pool<BaseAsset, QuoteAsset, ReferenceBaseAsset, ReferenceQuoteAsset>(
        sender: address,
        registry_id: ID,
        balance_manager_id: ID,
        test: &mut Scenario,
    ): ID {
        let target_pool_id = setup_pool_with_stable_fees<BaseAsset, QuoteAsset>(
            OWNER,
            registry_id,
            false,
            test,
        );
        let reference_pool_id = setup_reference_pool<ReferenceBaseAsset, ReferenceQuoteAsset>(
            sender,
            registry_id,
            balance_manager_id,
            100 * constants::float_scaling(),
            test,
        );
        set_time(0, test);
        add_deep_price_point<BaseAsset, QuoteAsset, ReferenceBaseAsset, ReferenceQuoteAsset>(
            sender,
            target_pool_id,
            reference_pool_id,
            test,
        );

        target_pool_id
    }

    /// Alice places a bid order, Bob places a swap_exact_amount order
    /// Make sure the assets returned to Bob are correct
    /// When swap is not fully filled, assets are returned correctly
    /// Make sure expired orders are skipped over
    fun test_swap_exact_not_fully_filled(
        is_bid: bool,
    ) {
        let mut test = begin(OWNER);
        let registry_id = setup_test(OWNER, &mut test);
        let balance_manager_id_alice = create_acct_and_share_with_funds(ALICE, 1000000 * constants::float_scaling(), &mut test);
        let pool_id = setup_pool_with_default_fees_and_reference_pool<SUI, USDC, SUI, DEEP>(ALICE, registry_id, balance_manager_id_alice, &mut test);

        let alice_client_order_id = 1;
        let alice_price = 3 * constants::float_scaling();
        let alice_quantity = 2 * constants::float_scaling();
        let expired_price = if (is_bid) {
            3 * constants::float_scaling()
        } else {
            1 * constants::float_scaling()
        };
        let expire_timestamp = constants::max_u64();
        let expire_timestamp_e = get_time(&mut test) + 100;
        let pay_with_deep = true;
        let residual = constants::lot_size() - 1;

        place_limit_order<SUI, USDC>(
            ALICE,
            pool_id,
            balance_manager_id_alice,
            alice_client_order_id,
            constants::no_restriction(),
            constants::self_matching_allowed(),
            alice_price,
            alice_quantity,
            is_bid,
            pay_with_deep,
            expire_timestamp,
            &mut test,
        );

        place_limit_order<SUI, USDC>(
            ALICE,
            pool_id,
            balance_manager_id_alice,
            alice_client_order_id,
            constants::no_restriction(),
            constants::self_matching_allowed(),
            expired_price,
            alice_quantity,
            is_bid,
            pay_with_deep,
            expire_timestamp_e,
            &mut test,
        );

        set_time(200, &mut test);

        let base_in = if (is_bid) {
            4 * constants::float_scaling() + residual
        } else {
            0
        };
        let quote_in = if (is_bid) {
            0
        } else {
            8 * constants::float_scaling() + 3 * residual
        };
        let deep_in = 2 * math::mul(constants::deep_multiplier(), constants::taker_fee()) + residual;

        let (base, quote, deep_required) = get_quantity_out<SUI, USDC>(
            pool_id,
            base_in,
            quote_in,
            &mut test,
        );

        let (base_2, quote_2, deep_required_2) = if (is_bid) {
            get_quote_quantity_out<SUI, USDC>(
                pool_id,
                base_in,
                &mut test,
            )
        } else {
            get_base_quantity_out<SUI, USDC>(
                pool_id,
                quote_in,
                &mut test,
            )
        };

        let (base_out, quote_out, deep_out) =
            if (is_bid) {
                place_swap_exact_base_for_quote<SUI, USDC>(
                    pool_id,
                    BOB,
                    base_in,
                    deep_in,
                    &mut test,
                )
            } else {
                place_swap_exact_quote_for_base<SUI, USDC>(
                    pool_id,
                    BOB,
                    quote_in,
                    deep_in,
                    &mut test,
                )
            };

        if (is_bid) {
            assert!(base_out.value() == 2 * constants::float_scaling() + residual, constants::e_order_info_mismatch());
            assert!(quote_out.value() == 6 * constants::float_scaling(), constants::e_order_info_mismatch());
        } else {
            assert!(base_out.value() == 2 * constants::float_scaling(), constants::e_order_info_mismatch());
            assert!(quote_out.value() == 2 * constants::float_scaling() + 3 * residual, constants::e_order_info_mismatch());
        };

        assert!(deep_out.value() == residual, constants::e_order_info_mismatch());
        assert!(base == base_2 && base == base_out.value(), constants::e_order_info_mismatch());
        assert!(quote == quote_2 && quote == quote_out.value(), constants::e_order_info_mismatch());
        assert!(deep_required == deep_required_2 && deep_required == deep_in - deep_out.value(), constants::e_order_info_mismatch());

        base_out.burn_for_testing();
        quote_out.burn_for_testing();
        deep_out.burn_for_testing();

        end(test);
    }

    /// Test getting the mid price of the order book
    /// Expired orders are skipped
    fun test_mid_price() {
        let mut test = begin(OWNER);
        let registry_id = setup_test(OWNER, &mut test);
        let balance_manager_id_alice = create_acct_and_share_with_funds(ALICE, 1000000 * constants::float_scaling(), &mut test);
        let pool_id = setup_pool_with_default_fees_and_reference_pool<SUI, USDC, SUI, DEEP>(ALICE, registry_id, balance_manager_id_alice, &mut test);

        let client_order_id = 1;
        let price_bid_1 = 1 * constants::float_scaling();
        let price_bid_best = 2 * constants::float_scaling();
        let price_bid_expired = 2_200_000_000;
        let price_ask_1 = 6 * constants::float_scaling();
        let price_ask_best = 5 * constants::float_scaling();
        let price_ask_expired = 3_200_000_000;
        let quantity = 1 * constants::float_scaling();
        let expire_timestamp = constants::max_u64();
        let expire_timestamp_e = get_time(&mut test) + 100;
        let pay_with_deep = true;
        let is_bid = true;

        place_limit_order<SUI, USDC>(
            ALICE,
            pool_id,
            balance_manager_id_alice,
            client_order_id,
            constants::no_restriction(),
            constants::self_matching_allowed(),
            price_bid_1,
            quantity,
            is_bid,
            pay_with_deep,
            expire_timestamp,
            &mut test,
        );

        place_limit_order<SUI, USDC>(
            ALICE,
            pool_id,
            balance_manager_id_alice,
            client_order_id,
            constants::no_restriction(),
            constants::self_matching_allowed(),
            price_bid_best,
            quantity,
            is_bid,
            pay_with_deep,
            expire_timestamp,
            &mut test,
        );

        place_limit_order<SUI, USDC>(
            ALICE,
            pool_id,
            balance_manager_id_alice,
            client_order_id,
            constants::no_restriction(),
            constants::self_matching_allowed(),
            price_bid_expired,
            quantity,
            is_bid,
            pay_with_deep,
            expire_timestamp_e,
            &mut test,
        );

        place_limit_order<SUI, USDC>(
            ALICE,
            pool_id,
            balance_manager_id_alice,
            client_order_id,
            constants::no_restriction(),
            constants::self_matching_allowed(),
            price_ask_1,
            quantity,
            !is_bid,
            pay_with_deep,
            expire_timestamp,
            &mut test,
        );

        place_limit_order<SUI, USDC>(
            ALICE,
            pool_id,
            balance_manager_id_alice,
            client_order_id,
            constants::no_restriction(),
            constants::self_matching_allowed(),
            price_ask_best,
            quantity,
            !is_bid,
            pay_with_deep,
            expire_timestamp,
            &mut test,
        );

        place_limit_order<SUI, USDC>(
            ALICE,
            pool_id,
            balance_manager_id_alice,
            client_order_id,
            constants::no_restriction(),
            constants::self_matching_allowed(),
            price_ask_expired,
            quantity,
            !is_bid,
            pay_with_deep,
            expire_timestamp_e,
            &mut test,
        );

        let expected_mid_price = (price_bid_expired + price_ask_expired) / 2;
        assert!(get_mid_price<SUI, USDC>(pool_id, &mut test) == expected_mid_price, constants::e_incorrect_mid_price());

        set_time(200, &mut test);
        let expected_mid_price = (price_bid_best + price_ask_best) / 2;
        assert!(get_mid_price<SUI, USDC>(pool_id, &mut test) == expected_mid_price, constants::e_incorrect_mid_price());

        end(test);
    }

    /// Places 3 orders at price 1, 2, 3 with quantity 1
    /// Market order of quantity 1.5 should fill one order completely, one partially, and one not at all
    /// Order 3 is fully filled for bid orders then ask market order
    /// Order 1 is fully filled for ask orders then bid market order
    /// Order 2 is partially filled for both
    fun test_market_order(
        is_bid: bool,
    ) {
        let mut test = begin(OWNER);
        let registry_id = setup_test(OWNER, &mut test);
        let balance_manager_id_alice = create_acct_and_share_with_funds(ALICE, 1000000 * constants::float_scaling(), &mut test);
        let pool_id = setup_pool_with_default_fees_and_reference_pool<SUI, USDC, SUI, DEEP>(ALICE, registry_id, balance_manager_id_alice, &mut test);

        let client_order_id = 1;
        let base_price = constants::float_scaling();
        let quantity = 1 * constants::float_scaling();
        let expire_timestamp = constants::max_u64();
        let pay_with_deep = true;
        let mut i = 0;
        let num_orders = 3;
        let partial_order_client_id = 2;
        let full_order_client_id = if (is_bid) {
            1
        } else {
            3
        };
        let mut partial_order_id = 0;
        let mut full_order_id = 0;

        while (i < num_orders) {
            let order_info = place_limit_order<SUI, USDC>(
                ALICE,
                pool_id,
                balance_manager_id_alice,
                client_order_id + i,
                constants::no_restriction(),
                constants::self_matching_allowed(),
                (client_order_id + i) * base_price,
                quantity,
                is_bid,
                pay_with_deep,
                expire_timestamp,
                &mut test,
            );
            if (order_info.client_order_id() == full_order_client_id) {
                full_order_id = order_info.order_id();
            };
            if (order_info.client_order_id() == partial_order_client_id) {
                partial_order_id = order_info.order_id();
            };
            i = i + 1;
        };

        let client_order_id = num_orders + 1;
        let fee_is_deep = true;
        let quantity_2 = 1_500_000_000;
        let price = if (is_bid) {
            constants::min_price()
        } else {
            constants::max_price()
        };

        let order_info = place_market_order<SUI, USDC>(
            ALICE,
            pool_id,
            balance_manager_id_alice,
            client_order_id,
            constants::self_matching_allowed(),
            quantity_2,
            !is_bid,
            pay_with_deep,
            &mut test,
        );

        let current_time = get_time(&mut test);
        let cumulative_quote_quantity = if (is_bid) {
            4_000_000_000
        } else {
            2_000_000_000
        };

        verify_order_info(
            &order_info,
            client_order_id,
            price,
            quantity_2,
            quantity_2,
            cumulative_quote_quantity,
            math::mul(
                quantity_2,
                math::mul(
                    constants::taker_fee(),
                    constants::deep_multiplier())
            ),
            fee_is_deep,
            constants::filled(),
            current_time,
        );

        borrow_and_verify_book_order<SUI, USDC>(
            pool_id,
            partial_order_id,
            is_bid,
            partial_order_client_id,
            quantity,
            500_000_000,
            constants::deep_multiplier(),
            0,
            constants::partially_filled(),
            constants::max_u64(),
            &mut test,
        );

        borrow_and_verify_book_order<SUI, USDC>(
            pool_id,
            full_order_id,
            is_bid,
            full_order_client_id,
            quantity,
            0,
            constants::deep_multiplier(),
            0,
            constants::live(),
            constants::max_u64(),
            &mut test,
        );

        end(test);
    }

    /// Test crossing num_orders orders with a single order
    /// Should be filled with the num_orders orders, with correct quantities
    /// Quantity of 1 for the first num_orders orders, quantity of num_orders for the last order
    fun test_crossing_multiple(
        is_bid: bool,
        num_orders: u64,
    ) {
        let mut test = begin(OWNER);
        let registry_id = setup_test(OWNER, &mut test);
        let balance_manager_id_alice = create_acct_and_share_with_funds(ALICE, 1000000 * constants::float_scaling(), &mut test);
        let pool_id = setup_pool_with_default_fees_and_reference_pool<SUI, USDC, SUI, DEEP>(ALICE, registry_id, balance_manager_id_alice, &mut test);

        let client_order_id = 1;
        let price = 2 * constants::float_scaling();
        let quantity = 1 * constants::float_scaling();
        let expire_timestamp = constants::max_u64();
        let pay_with_deep = true;

        let mut i = 0;
        while (i < num_orders) {
            place_limit_order<SUI, USDC>(
                ALICE,
                pool_id,
                balance_manager_id_alice,
                client_order_id,
                constants::no_restriction(),
                constants::self_matching_allowed(),
                price,
                quantity,
                is_bid,
                pay_with_deep,
                expire_timestamp,
                &mut test,
            );
            i = i + 1;
        };

        let client_order_id = 3;
        let price = if (is_bid) {
            1 * constants::float_scaling()
        } else {
            3 * constants::float_scaling()
        };

        let order_info = place_limit_order<SUI, USDC>(
            ALICE,
            pool_id,
            balance_manager_id_alice,
            client_order_id,
            constants::no_restriction(),
            constants::self_matching_allowed(),
            price,
            num_orders * quantity,
            !is_bid,
            pay_with_deep,
            expire_timestamp,
            &mut test,
        );

        verify_order_info(
            &order_info,
            client_order_id,
            price,
            num_orders * quantity,
            num_orders * quantity,
            2 * num_orders * quantity,
            num_orders * math::mul(constants::taker_fee(), constants::deep_multiplier()),
            true,
            constants::filled(),
            expire_timestamp,
        );

        end(test);
    }

    /// Test fill or kill order that crosses with an order that's smaller in quantity
    /// Should error with EFOKOrderCannotBeFullyFilled if order cannot be fully filled
    /// Should fill correctly if order can be fully filled
    /// First order has quantity 1, second order has quantity 2 for incorrect fill
    /// First two orders have quantity 1, third order is quantity 2 for correct fill
    fun test_fill_or_kill(
        is_bid: bool,
        order_can_be_filled: bool,
    ) {
        let mut test = begin(OWNER);
        let registry_id = setup_test(OWNER, &mut test);
        let balance_manager_id_alice = create_acct_and_share_with_funds(ALICE, 1000000 * constants::float_scaling(), &mut test);
        let pool_id = setup_pool_with_default_fees_and_reference_pool<SUI, USDC, SUI, DEEP>(ALICE, registry_id, balance_manager_id_alice, &mut test);

        let client_order_id = 1;
        let price = 2 * constants::float_scaling();
        let quantity = 1 * constants::float_scaling();
        let expire_timestamp = constants::max_u64();
        let pay_with_deep = true;
        let quantity_multiplier = 2;
        let mut num_orders = if (order_can_be_filled) {
            quantity_multiplier
        } else {
            1
        };

        while (num_orders > 0) {
            place_limit_order<SUI, USDC>(
                ALICE,
                pool_id,
                balance_manager_id_alice,
                client_order_id,
                constants::no_restriction(),
                constants::self_matching_allowed(),
                price,
                quantity,
                is_bid,
                pay_with_deep,
                expire_timestamp,
                &mut test,
            );
            num_orders = num_orders - 1;
        };

        // Place a second order that crosses with the first i orders
        let client_order_id = 2;
        let price = if (is_bid) {
            1 * constants::float_scaling()
        } else {
            3 * constants::float_scaling()
        };

        let order_info = place_limit_order<SUI, USDC>(
            ALICE,
            pool_id,
            balance_manager_id_alice,
            client_order_id,
            constants::fill_or_kill(),
            constants::self_matching_allowed(),
            price,
            quantity_multiplier * quantity,
            !is_bid,
            pay_with_deep,
            expire_timestamp,
            &mut test,
        );

        verify_order_info(
            &order_info,
            client_order_id,
            price,
            quantity_multiplier * quantity,
            quantity_multiplier * quantity,
            2 * quantity_multiplier * quantity,
            quantity_multiplier * math::mul(constants::taker_fee(), constants::deep_multiplier()),
            true,
            constants::filled(),
            expire_timestamp,
        );

        end(test);
    }

    /// Test post only order that crosses with another order
    /// Should error with EPOSTOrderCrossesOrderbook
    fun test_post_only(
        is_bid: bool,
        crosses_order: bool,
    ) {
        let mut test = begin(OWNER);
        let registry_id = setup_test(OWNER, &mut test);
        let balance_manager_id_alice = create_acct_and_share_with_funds(ALICE, 1000000 * constants::float_scaling(), &mut test);
        let pool_id = setup_pool_with_default_fees_and_reference_pool<SUI, USDC, SUI, DEEP>(ALICE, registry_id, balance_manager_id_alice, &mut test);

        let client_order_id = 1;
        let order_type = constants::post_only();
        let price = 2 * constants::float_scaling();
        let quantity = 1 * constants::float_scaling();
        let expire_timestamp = constants::max_u64();
        let pay_with_deep = true;

        place_limit_order<SUI, USDC>(
            ALICE,
            pool_id,
            balance_manager_id_alice,
            client_order_id,
            order_type,
            constants::self_matching_allowed(),
            price,
            quantity,
            is_bid,
            pay_with_deep,
            expire_timestamp,
            &mut test,
        );

        // Place a second order that crosses with the first order
        let client_order_id = 2;
        let price = if ((is_bid && crosses_order) || (!is_bid && !crosses_order)) {
            1 * constants::float_scaling()
        } else {
            3 * constants::float_scaling()
        };

        place_limit_order<SUI, USDC>(
            ALICE,
            pool_id,
            balance_manager_id_alice,
            client_order_id,
            order_type,
            constants::self_matching_allowed(),
            price,
            quantity,
            !is_bid,
            pay_with_deep,
            expire_timestamp,
            &mut test,
        );

        end(test);
    }

    #[test, expected_failure(abort_code = ::deepbook::order_info::EInvalidOrderType)]
    /// placing an order > MAX_RESTRICTIONS should fail
    fun place_order_max_restrictions_e() {
        let mut test = begin(OWNER);
        let registry_id = setup_test(OWNER, &mut test);
        let balance_manager_id_alice = create_acct_and_share_with_funds(ALICE, 1000000 * constants::float_scaling(), &mut test);
        let pool_id = setup_pool_with_default_fees_and_reference_pool<SUI, USDC, SUI, DEEP>(ALICE, registry_id, balance_manager_id_alice, &mut test);
        let client_order_id = 1;
        let order_type = constants::max_restriction() + 1;
        let price = 2 * constants::float_scaling();
        let quantity = 1 * constants::float_scaling();
        let expire_timestamp = constants::max_u64();
        let pay_with_deep = true;

        place_limit_order<SUI, USDC>(
            ALICE,
            pool_id,
            balance_manager_id_alice,
            client_order_id,
            order_type,
            constants::self_matching_allowed(),
            price,
            quantity,
            true,
            pay_with_deep,
            expire_timestamp,
            &mut test,
        );
        end(test);
    }

    #[test, expected_failure(abort_code = ::deepbook::big_vector::ENotFound)]
    /// Trying to cancel a cancelled order should fail
    fun place_and_cancel_order_empty_e() {
        let mut test = begin(OWNER);
        let registry_id = setup_test(OWNER, &mut test);
        let balance_manager_id_alice = create_acct_and_share_with_funds(ALICE, 1000000 * constants::float_scaling(), &mut test);
        let pool_id = setup_pool_with_default_fees_and_reference_pool<SUI, USDC, SUI, DEEP>(ALICE, registry_id, balance_manager_id_alice, &mut test);

        let client_order_id = 1;
        let order_type = constants::no_restriction();
        let price = 2 * constants::float_scaling();
        let alice_quantity = 1 * constants::float_scaling();
        let expire_timestamp = constants::max_u64();
        let is_bid = true;
        let pay_with_deep = true;

        let placed_order_id = place_limit_order<SUI, USDC>(
            ALICE,
            pool_id,
            balance_manager_id_alice,
            client_order_id, // client_order_id
            order_type,
            constants::self_matching_allowed(),
            price, // price
            alice_quantity, // quantity
            is_bid,
            pay_with_deep,
            expire_timestamp, // no expiration
            &mut test,
        ).order_id();
        cancel_order<SUI, USDC>(
            ALICE,
            pool_id,
            balance_manager_id_alice,
            placed_order_id,
            &mut test
        );
        cancel_order<SUI, USDC>(
            ALICE,
            pool_id,
            balance_manager_id_alice,
            placed_order_id,
            &mut test
        );
        end(test);
    }

    #[test, expected_failure(abort_code = ::deepbook::order_info::EInvalidExpireTimestamp)]
    /// Trying to place an order that's expiring should fail
    fun place_order_expired_order_skipped() {
        let mut test = begin(OWNER);
        let registry_id = setup_test(OWNER, &mut test);
        let balance_manager_id_alice = create_acct_and_share_with_funds(ALICE, 1000000 * constants::float_scaling(), &mut test);
        let pool_id = setup_pool_with_default_fees_and_reference_pool<SUI, USDC, SUI, DEEP>(ALICE, registry_id, balance_manager_id_alice, &mut test);
        set_time(100, &mut test);

        let client_order_id = 1;
        let order_type = constants::no_restriction();
        let price = 2 * constants::float_scaling();
        let quantity = 1 * constants::float_scaling();
        let expire_timestamp = 0;
        let is_bid = true;
        let pay_with_deep = true;

        place_limit_order<SUI, USDC>(
            ALICE,
            pool_id,
            balance_manager_id_alice,
            client_order_id,
            order_type,
            constants::self_matching_allowed(),
            price,
            quantity,
            is_bid,
            pay_with_deep,
            expire_timestamp,
            &mut test,
        );
        end(test);
    }

    fun test_cancel_all_orders(
        is_bid: bool,
    ) {
        let mut test = begin(OWNER);
        let registry_id = setup_test(OWNER, &mut test);
        let balance_manager_id_alice = create_acct_and_share_with_funds(ALICE, 1000000 * constants::float_scaling(), &mut test);
        let pool_id = setup_pool_with_default_fees_and_reference_pool<SUI, USDC, SUI, DEEP>(ALICE, registry_id, balance_manager_id_alice, &mut test);

        let client_order_id = 1;
        let order_type = constants::no_restriction();
        let price = 2 * constants::float_scaling();
        let quantity = 1 * constants::float_scaling();
        let expire_timestamp = constants::max_u64();
        let pay_with_deep = true;

        let order_info_1 = place_limit_order<SUI, USDC>(
            ALICE,
            pool_id,
            balance_manager_id_alice,
            client_order_id,
            order_type,
            constants::self_matching_allowed(),
            price,
            quantity,
            is_bid,
            pay_with_deep,
            expire_timestamp,
            &mut test,
        );

        let client_order_id = 2;

        let order_info_2 = place_limit_order<SUI, USDC>(
            ALICE,
            pool_id,
            balance_manager_id_alice,
            client_order_id,
            order_type,
            constants::self_matching_allowed(),
            price,
            quantity,
            is_bid,
            pay_with_deep,
            expire_timestamp,
            &mut test,
        );

        borrow_order_ok<SUI, USDC>(
            pool_id,
            order_info_1.order_id(),
            &mut test,
        );

        borrow_order_ok<SUI, USDC>(
            pool_id,
            order_info_2.order_id(),
            &mut test,
        );

        cancel_all_orders<SUI, USDC>(
            pool_id,
            ALICE,
            balance_manager_id_alice,
            &mut test
        );

        borrow_order_ok<SUI, USDC>(
            pool_id,
            order_info_1.order_id(),
            &mut test,
        );
        end(test);
    }

    /// Alice places a bid order, Bob places a swap_exact_amount order
    /// Make sure the assets returned to Bob are correct
    /// Make sure expired orders are skipped over
    fun test_swap_exact_amount(
        is_bid: bool,
    ) {
        let mut test = begin(OWNER);
        let registry_id = setup_test(OWNER, &mut test);
        let balance_manager_id_alice = create_acct_and_share_with_funds(ALICE, 1000000 * constants::float_scaling(), &mut test);
        let pool_id = setup_pool_with_default_fees_and_reference_pool<SUI, USDC, SUI, DEEP>(ALICE, registry_id, balance_manager_id_alice, &mut test);

        let alice_client_order_id = 1;
        let alice_price = 2 * constants::float_scaling();
        let alice_quantity = 2 * constants::float_scaling();
        let expired_price = if (is_bid) {
            3 * constants::float_scaling()
        } else {
            1 * constants::float_scaling()
        };
        let expire_timestamp = constants::max_u64();
        let expire_timestamp_e = get_time(&mut test) + 100;
        let pay_with_deep = true;
        let residual = constants::lot_size() - 1;

        place_limit_order<SUI, USDC>(
            ALICE,
            pool_id,
            balance_manager_id_alice,
            alice_client_order_id,
            constants::no_restriction(),
            constants::self_matching_allowed(),
            alice_price,
            alice_quantity,
            is_bid,
            pay_with_deep,
            expire_timestamp,
            &mut test,
        );

        place_limit_order<SUI, USDC>(
            ALICE,
            pool_id,
            balance_manager_id_alice,
            alice_client_order_id,
            constants::no_restriction(),
            constants::self_matching_allowed(),
            expired_price,
            alice_quantity,
            is_bid,
            pay_with_deep,
            expire_timestamp_e,
            &mut test,
        );

        set_time(200, &mut test);

        let base_in = if (is_bid) {
            1 * constants::float_scaling() + residual
        } else {
            0
        };
        let quote_in = if (is_bid) {
            0
        } else {
            2 * constants::float_scaling() + 2 * residual
        };
        let deep_in = math::mul(constants::deep_multiplier(), constants::taker_fee()) + residual;

        let (base, quote, deep_required) = get_quantity_out<SUI, USDC>(
            pool_id,
            base_in,
            quote_in,
            &mut test,
        );

        let (base_2, quote_2, deep_required_2) = if (is_bid) {
            get_quote_quantity_out<SUI, USDC>(
                pool_id,
                base_in,
                &mut test,
            )
        } else {
            get_base_quantity_out<SUI, USDC>(
                pool_id,
                quote_in,
                &mut test,
            )
        };

        let (base_out, quote_out, deep_out) =
            if (is_bid) {
                place_swap_exact_base_for_quote<SUI, USDC>(
                    pool_id,
                    BOB,
                    base_in,
                    deep_in,
                    &mut test,
                )
            } else {
                place_swap_exact_quote_for_base<SUI, USDC>(
                    pool_id,
                    BOB,
                    quote_in,
                    deep_in,
                    &mut test,
                )
            };

        if (is_bid) {
            assert!(base_out.value() == residual, constants::e_order_info_mismatch());
            assert!(quote_out.value() == 2 * constants::float_scaling(), constants::e_order_info_mismatch());
        } else {
            assert!(base_out.value() == 1 * constants::float_scaling(), constants::e_order_info_mismatch());
            assert!(quote_out.value() == 2 * residual, constants::e_order_info_mismatch());
        };

        assert!(deep_out.value() == residual, constants::e_order_info_mismatch());
        assert!(base == base_2 && base == base_out.value(), constants::e_order_info_mismatch());
        assert!(quote == quote_2 && quote == quote_out.value(), constants::e_order_info_mismatch());
        assert!(deep_required == deep_required_2 && deep_required == deep_in - deep_out.value(), constants::e_order_info_mismatch());

        base_out.burn_for_testing();
        quote_out.burn_for_testing();
        deep_out.burn_for_testing();

        end(test);
    }

    /// Alice places a bid/ask order
    /// Alice then places an ask/bid order that crosses with that order with cancel_taker option
    /// Order should be rejected.
    fun test_self_matching_cancel_taker(
        is_bid: bool,
    ) {
        let mut test = begin(OWNER);
        let registry_id = setup_test(OWNER, &mut test);
        let balance_manager_id_alice = create_acct_and_share_with_funds(ALICE, 1000000 * constants::float_scaling(), &mut test);
        let pool_id = setup_pool_with_default_fees_and_reference_pool<SUI, USDC, SUI, DEEP>(ALICE, registry_id, balance_manager_id_alice, &mut test);

        let bid_client_order_id = 1;
        let ask_client_order_id = 2;
        let order_type = constants::no_restriction();
        let price_1 = 2 * constants::float_scaling();
        let price_2 = if (is_bid) {
            1 * constants::float_scaling()
        } else {
            3 * constants::float_scaling()
        };
        let quantity = 1 * constants::float_scaling();
        let expire_timestamp = constants::max_u64();
        let pay_with_deep = true;
        let fee_is_deep = true;

        let order_info_1 = place_limit_order<SUI, USDC>(
            ALICE,
            pool_id,
            balance_manager_id_alice,
            bid_client_order_id,
            order_type,
            constants::self_matching_allowed(),
            price_1,
            quantity,
            is_bid,
            pay_with_deep,
            expire_timestamp,
            &mut test,
        );

        verify_order_info(
            &order_info_1,
            bid_client_order_id,
            price_1,
            quantity,
            0,
            0,
            0,
            fee_is_deep,
            constants::live(),
            expire_timestamp,
        );

        place_limit_order<SUI, USDC>(
            ALICE,
            pool_id,
            balance_manager_id_alice,
            ask_client_order_id,
            order_type,
            constants::cancel_taker(),
            price_2,
            quantity,
            !is_bid,
            pay_with_deep,
            expire_timestamp,
            &mut test,
        );

        end(test);
    }

    /// Alice places a bid/ask order
    /// Alice then places an ask/bid order that crosses with that order with cancel_maker option
    /// Maker order should be removed, with the new order placed successfully.
    fun test_self_matching_cancel_maker(
        is_bid: bool,
    ) {
        let mut test = begin(OWNER);
        let registry_id = setup_test(OWNER, &mut test);
        let balance_manager_id_alice = create_acct_and_share_with_funds(ALICE, 1000000 * constants::float_scaling(), &mut test);
        let pool_id = setup_pool_with_default_fees_and_reference_pool<SUI, USDC, SUI, DEEP>(ALICE, registry_id, balance_manager_id_alice, &mut test);

        let client_order_id_1 = 1;
        let client_order_id_2 = 2;
        let order_type = constants::no_restriction();
        let price_1 = 2 * constants::float_scaling();
        let price_2 = if (is_bid) {
            1 * constants::float_scaling()
        } else {
            3 * constants::float_scaling()
        };
        let quantity = 1 * constants::float_scaling();
        let expire_timestamp = constants::max_u64();
        let pay_with_deep = true;
        let fee_is_deep = true;

        let order_info_1 = place_limit_order<SUI, USDC>(
            ALICE,
            pool_id,
            balance_manager_id_alice,
            client_order_id_1,
            order_type,
            constants::self_matching_allowed(),
            price_1,
            quantity,
            is_bid,
            pay_with_deep,
            expire_timestamp,
            &mut test,
        );

        verify_order_info(
            &order_info_1,
            client_order_id_1,
            price_1,
            quantity,
            0,
            0,
            0,
            fee_is_deep,
            constants::live(),
            expire_timestamp,
        );

        let order_info_2 = place_limit_order<SUI, USDC>(
            ALICE,
            pool_id,
            balance_manager_id_alice,
            client_order_id_2,
            order_type,
            constants::cancel_maker(),
            price_2,
            quantity,
            !is_bid,
            pay_with_deep,
            expire_timestamp,
            &mut test,
        );

        verify_order_info(
            &order_info_2,
            client_order_id_2,
            price_2,
            quantity,
            0,
            0,
            0,
            fee_is_deep,
            constants::live(),
            expire_timestamp,
        );

        borrow_order_ok<SUI, USDC>(
            pool_id,
            order_info_1.order_id(),
            &mut test,
        );

        end(test);
    }

    fun place_with_price_quantity(
        price: u64,
        quantity: u64,
    ) {
        let mut test = begin(OWNER);
        let registry_id = setup_test(OWNER, &mut test);
        let balance_manager_id_alice = create_acct_and_share_with_funds(ALICE, 1000000 * constants::float_scaling(), &mut test);
        let pool_id = setup_pool_with_default_fees_and_reference_pool<SUI, USDC, SUI, DEEP>(ALICE, registry_id, balance_manager_id_alice, &mut test);

        let client_order_id = 1;
        let order_type = constants::no_restriction();
        let expire_timestamp = constants::max_u64();
        let pay_with_deep = true;

        place_limit_order<SUI, USDC>(
            ALICE,
            pool_id,
            balance_manager_id_alice,
            client_order_id,
            order_type,
            constants::self_matching_allowed(),
            price,
            quantity,
            true,
            pay_with_deep,
            expire_timestamp,
            &mut test,
        );
        end(test);
    }

    fun partial_fill_order(
        is_bid: bool,
        order_type: u8,
        alice_quantity: u64,
        expected_executed_quantity: u64,
        expected_cumulative_quote_quantity: u64,
        expected_paid_fees: u64,
        expected_status: u8,
    ) {
        let mut test = begin(OWNER);
        let registry_id = setup_test(OWNER, &mut test);
        let balance_manager_id_alice = create_acct_and_share_with_funds(ALICE, 1000000 * constants::float_scaling(), &mut test);
        let pool_id = setup_pool_with_default_fees_and_reference_pool<SUI, USDC, SUI, DEEP>(ALICE, registry_id, balance_manager_id_alice, &mut test);
        let balance_manager_id_bob = create_acct_and_share_with_funds(BOB, 1000000 * constants::float_scaling(), &mut test);

        let alice_client_order_id = 1;
        let alice_price = 2 * constants::float_scaling();
        let expire_timestamp = constants::max_u64();
        let pay_with_deep = true;

        place_limit_order<SUI, USDC>(
            ALICE,
            pool_id,
            balance_manager_id_alice,
            alice_client_order_id,
            constants::no_restriction(),
            constants::self_matching_allowed(),
            alice_price,
            alice_quantity,
            is_bid,
            pay_with_deep,
            expire_timestamp,
            &mut test,
        );

        let bob_client_order_id = 2;
        let bob_price = 2 * constants::float_scaling();
        let bob_quantity = 2 * alice_quantity;

        let bob_order_info = place_limit_order<SUI, USDC>(
            BOB,
            pool_id,
            balance_manager_id_bob,
            bob_client_order_id,
            order_type,
            constants::self_matching_allowed(),
            bob_price,
            bob_quantity,
            !is_bid,
            pay_with_deep,
            expire_timestamp,
            &mut test,
        );

        let fee_is_deep = true;

        verify_order_info(
            &bob_order_info,
            bob_client_order_id,
            bob_price,
            bob_quantity,
            expected_executed_quantity,
            expected_cumulative_quote_quantity,
            expected_paid_fees,
            fee_is_deep,
            expected_status,
            expire_timestamp,
        );

        borrow_order_ok<SUI, USDC>(
            pool_id,
            bob_order_info.order_id(),
            &mut test,
        );

        end(test);
    }

    /// Place normal ask order, then try to fill full order.
    /// Alice places first order, Bob places second order.
    fun place_then_fill(
        is_stable: bool,
        is_bid: bool,
        order_type: u8,
        alice_quantity: u64,
        expected_executed_quantity: u64,
        expected_cumulative_quote_quantity: u64,
        expected_paid_fees: u64,
        expected_status: u8,
    ) {
        let mut test = begin(OWNER);
        let registry_id = setup_test(OWNER, &mut test);
        let balance_manager_id_alice = create_acct_and_share_with_funds(ALICE, 1000000 * constants::float_scaling(), &mut test);
        let pool_id = if (is_stable) {
            setup_pool_with_stable_fees_and_reference_pool<SUI, USDC, SUI, DEEP>(ALICE, registry_id, balance_manager_id_alice, &mut test)
        } else {
            setup_pool_with_default_fees_and_reference_pool<SUI, USDC, SUI, DEEP>(ALICE, registry_id, balance_manager_id_alice, &mut test)
        };
        let balance_manager_id_bob = create_acct_and_share_with_funds(BOB, 1000000 * constants::float_scaling(), &mut test);

        let alice_client_order_id = 1;
        let alice_price = 2 * constants::float_scaling();
        let expire_timestamp = constants::max_u64();
        let pay_with_deep = true;

        place_limit_order<SUI, USDC>(
            ALICE,
            pool_id,
            balance_manager_id_alice,
            alice_client_order_id,
            constants::no_restriction(),
            constants::self_matching_allowed(),
            alice_price,
            alice_quantity,
            is_bid,
            pay_with_deep,
            expire_timestamp,
            &mut test,
        );

        let bob_client_order_id = 2;
        let bob_price = if (is_bid) {
            1 * constants::float_scaling()
        } else {
            3 * constants::float_scaling()
        };
        let bob_quantity = alice_quantity;

        let bob_order_info = place_limit_order<SUI, USDC>(
            BOB,
            pool_id,
            balance_manager_id_bob,
            bob_client_order_id,
            order_type,
            constants::self_matching_allowed(),
            bob_price,
            bob_quantity,
            !is_bid,
            pay_with_deep,
            expire_timestamp,
            &mut test,
        );

        let expire_timestamp = constants::max_u64();
        let fee_is_deep = true;

        verify_order_info(
            &bob_order_info,
            bob_client_order_id,
            bob_price,
            bob_quantity,
            expected_executed_quantity,
            expected_cumulative_quote_quantity,
            expected_paid_fees,
            fee_is_deep,
            expected_status,
            expire_timestamp,
        );
        end(test);
    }

    /// Place normal ask order, then try to place without filling.
    /// Alice places first order, Bob places second order.
    fun place_then_no_fill(
        is_bid: bool,
        order_type: u8,
        expected_executed_quantity: u64,
        expected_cumulative_quote_quantity: u64,
        expected_paid_fees: u64,
        expected_status: u8,
    ) {
        let mut test = begin(OWNER);
        let registry_id = setup_test(OWNER, &mut test);
        let balance_manager_id_alice = create_acct_and_share_with_funds(ALICE, 1000000 * constants::float_scaling(), &mut test);
        let pool_id = setup_pool_with_default_fees_and_reference_pool<SUI, USDC, SUI, DEEP>(ALICE, registry_id, balance_manager_id_alice, &mut test);
        let balance_manager_id_bob = create_acct_and_share_with_funds(BOB, 1000000 * constants::float_scaling(), &mut test);

        let client_order_id = 1;
        let price = 2 * constants::float_scaling();
        let quantity = 1 * constants::float_scaling();
        let expire_timestamp = constants::max_u64();
        let pay_with_deep = true;

        place_limit_order<SUI, USDC>(
            ALICE,
            pool_id,
            balance_manager_id_alice,
            client_order_id,
            constants::no_restriction(),
            constants::self_matching_allowed(),
            price,
            quantity,
            is_bid,
            pay_with_deep,
            expire_timestamp,
            &mut test,
        );

        let client_order_id = 2;
        let price = if (is_bid) {
            3 * constants::float_scaling()
        } else {
            1 * constants::float_scaling()
        };

        let order_info = place_limit_order<SUI, USDC>(
            BOB,
            pool_id,
            balance_manager_id_bob,
            client_order_id,
            order_type,
            constants::self_matching_allowed(),
            price,
            quantity,
            !is_bid,
            pay_with_deep,
            expire_timestamp,
            &mut test,
        );

        let quantity = 1 * constants::float_scaling();
        let expire_timestamp = constants::max_u64();
        let fee_is_deep = true;

        verify_order_info(
            &order_info,
            client_order_id,
            price,
            quantity,
            expected_executed_quantity,
            expected_cumulative_quote_quantity,
            expected_paid_fees,
            fee_is_deep,
            expected_status,
            expire_timestamp,
        );

        cancel_order<SUI, USDC>(
            BOB,
            pool_id,
            balance_manager_id_bob,
            order_info.order_id(),
            &mut test
        );
        end(test);
    }

    /// Trying to fill an order that's expired on the book should remove order.
    /// New order should be placed successfully.
    /// Old order no longer exists.
    fun place_order_expire_timestamp_e(
        is_bid: bool,
        order_type: u8,
        expected_executed_quantity: u64,
        expected_cumulative_quote_quantity: u64,
        expected_paid_fees: u64,
        expected_status: u8,
    ) {
        let mut test = begin(OWNER);
        let registry_id = setup_test(OWNER, &mut test);
        let balance_manager_id_alice = create_acct_and_share_with_funds(ALICE, 1000000 * constants::float_scaling(), &mut test);
        let pool_id = setup_pool_with_default_fees_and_reference_pool<SUI, USDC, SUI, DEEP>(ALICE, registry_id, balance_manager_id_alice, &mut test);
        let balance_manager_id_bob = create_acct_and_share_with_funds(BOB, 1000000 * constants::float_scaling(), &mut test);

        let client_order_id = 1;
        let price = 2 * constants::float_scaling();
        let quantity = 1 * constants::float_scaling();
        let pay_with_deep = true;
        let fee_is_deep = true;
        let expire_timestamp = get_time(&mut test) + 100;

        let order_info_alice = place_limit_order<SUI, USDC>(
            ALICE,
            pool_id,
            balance_manager_id_alice,
            client_order_id,
            constants::no_restriction(),
            constants::self_matching_allowed(),
            price,
            quantity,
            is_bid,
            pay_with_deep,
            expire_timestamp,
            &mut test,
        );

        set_time(200, &mut test);
        verify_order_info(
            &order_info_alice,
            client_order_id,
            price,
            quantity,
            expected_executed_quantity,
            expected_cumulative_quote_quantity,
            expected_paid_fees,
            fee_is_deep,
            expected_status,
            expire_timestamp,
        );

        let client_order_id = 2;
        let price = if (is_bid) {
            1 * constants::float_scaling()
        } else {
            3 * constants::float_scaling()
        };
        let expire_timestamp = constants::max_u64();

        let order_info_bob = place_limit_order<SUI, USDC>(
            BOB,
            pool_id,
            balance_manager_id_bob,
            client_order_id,
            order_type,
            constants::self_matching_allowed(),
            price,
            quantity,
            !is_bid,
            pay_with_deep,
            expire_timestamp,
            &mut test,
        );

        let quantity = 1 * constants::float_scaling();
        let expire_timestamp = constants::max_u64();

        verify_order_info(
            &order_info_bob,
            client_order_id,
            price,
            quantity,
            expected_executed_quantity,
            expected_cumulative_quote_quantity,
            expected_paid_fees,
            fee_is_deep,
            expected_status,
            expire_timestamp,
        );

        borrow_and_verify_book_order<SUI, USDC>(
            pool_id,
            order_info_bob.order_id(),
            !is_bid,
            client_order_id,
            quantity,
            expected_executed_quantity,
            order_info_bob.order_deep_price().deep_per_asset(),
            test.ctx().epoch(),
            expected_status,
            expire_timestamp,
            &mut test,
        );

        borrow_order_ok<SUI, USDC>(
            pool_id,
            order_info_alice.order_id(),
            &mut test,
        );
        end(test);
    }

    /// Test to place a limit order, verify the order info and order in the book
    fun place_order_ok(
        is_bid: bool,
    ) {
        let mut test = begin(OWNER);
        let registry_id = setup_test(OWNER, &mut test);
        let balance_manager_id_alice = create_acct_and_share_with_funds(ALICE, 1000000 * constants::float_scaling(), &mut test);
        let pool_id = setup_pool_with_default_fees_and_reference_pool<SUI, USDC, SUI, DEEP>(ALICE, registry_id, balance_manager_id_alice, &mut test);

        // variables to input into order
        let client_order_id = 1;
        let order_type = constants::no_restriction();
        let price = 2 * constants::float_scaling();
        let quantity = 1 * constants::float_scaling();
        let expire_timestamp = constants::max_u64();

        // variables expected from OrderInfo and Order
        let status = constants::live();
        let executed_quantity = 0;
        let cumulative_quote_quantity = 0;
        let paid_fees = 0;
        let fee_is_deep = true;
        let pay_with_deep = true;

        let order_info = &place_limit_order<SUI, USDC>(
            ALICE,
            pool_id,
            balance_manager_id_alice,
            client_order_id,
            order_type,
            constants::self_matching_allowed(),
            price,
            quantity,
            is_bid,
            pay_with_deep,
            expire_timestamp,
            &mut test,
        );

        verify_order_info(
            order_info,
            client_order_id,
            price,
            quantity,
            executed_quantity,
            cumulative_quote_quantity,
            paid_fees,
            fee_is_deep,
            status,
            expire_timestamp,
        );

        borrow_and_verify_book_order<SUI, USDC>(
            pool_id,
            order_info.order_id(),
            is_bid,
            client_order_id,
            quantity,
            executed_quantity,
            order_info.order_deep_price().deep_per_asset(),
            test.ctx().epoch(),
            status,
            expire_timestamp,
            &mut test,
        );
        end(test);
    }

    /// Test placing and cancelling a limit order.
    fun place_and_cancel_order_ok(
        is_bid: bool,
    ) {
        let mut test = begin(OWNER);
        let registry_id = setup_test(OWNER, &mut test);
        let balance_manager_id_alice = create_acct_and_share_with_funds(ALICE, 1000000 * constants::float_scaling(), &mut test);
        let pool_id = setup_pool_with_default_fees_and_reference_pool<SUI, USDC, SUI, DEEP>(ALICE, registry_id, balance_manager_id_alice, &mut test);

        // variables to input into order
        let client_order_id = 1;
        let order_type = constants::no_restriction();
        let price = 2 * constants::float_scaling();
        let quantity = 1 * constants::float_scaling();
        let expire_timestamp = constants::max_u64();
        let pay_with_deep = true;
        let executed_quantity = 0;
        let cumulative_quote_quantity = 0;
        let paid_fees = 0;
        let fee_is_deep = true;
        let status = constants::live();

        let order_info = place_limit_order<SUI, USDC>(
            ALICE,
            pool_id,
            balance_manager_id_alice,
            client_order_id,
            order_type,
            constants::self_matching_allowed(),
            price,
            quantity,
            is_bid,
            pay_with_deep,
            expire_timestamp,
            &mut test,
        );

        verify_order_info(
            &order_info,
            client_order_id,
            price,
            quantity,
            executed_quantity,
            cumulative_quote_quantity,
            paid_fees,
            fee_is_deep,
            status,
            expire_timestamp,
        );

        cancel_order<SUI, USDC>(
            ALICE,
            pool_id,
            balance_manager_id_alice,
            order_info.order_id(),
            &mut test
        );
        end(test);
    }

    /// Helper, verify OrderInfo fields
    fun verify_order_info(
        order_info: &OrderInfo,
        client_order_id: u64,
        price: u64,
        original_quantity: u64,
        executed_quantity: u64,
        cumulative_quote_quantity: u64,
        paid_fees: u64,
        fee_is_deep: bool,
        status: u8,
        expire_timestamp: u64,
    ) {
        assert!(order_info.client_order_id() == client_order_id, constants::e_order_info_mismatch());
        assert!(order_info.price() == price, constants::e_order_info_mismatch());
        assert!(order_info.original_quantity() == original_quantity, constants::e_order_info_mismatch());
        assert!(order_info.executed_quantity() == executed_quantity, constants::e_order_info_mismatch());
        assert!(order_info.cumulative_quote_quantity() == cumulative_quote_quantity, constants::e_order_info_mismatch());
        assert!(order_info.paid_fees() == paid_fees, constants::e_order_info_mismatch());
        assert!(order_info.fee_is_deep() == fee_is_deep, constants::e_order_info_mismatch());
        assert!(order_info.status() == status, constants::e_order_info_mismatch());
        assert!(order_info.expire_timestamp() == expire_timestamp, constants::e_order_info_mismatch());
    }

    /// Helper, borrow orderbook and verify an order.
    fun borrow_and_verify_book_order<BaseAsset, QuoteAsset>(
        pool_id: ID,
        book_order_id: u128,
        is_bid: bool,
        client_order_id: u64,
        quantity: u64,
        filled_quantity: u64,
        deep_per_asset: u64,
        epoch: u64,
        status: u8,
        expire_timestamp: u64,
        test: &mut Scenario,
    ) {
        test.next_tx(@0x1);
        let pool = test.take_shared_by_id<Pool<BaseAsset, QuoteAsset>>(pool_id);
        let order = borrow_orderbook(&pool, is_bid).borrow(book_order_id);
        verify_book_order(
            order,
            book_order_id,
            client_order_id,
            quantity,
            filled_quantity,
            deep_per_asset,
            epoch,
            status,
            expire_timestamp,
        );
        return_shared(pool);
    }

    /// Internal function to borrow orderbook to ensure order exists
    fun borrow_order_ok<BaseAsset, QuoteAsset>(
        pool_id: ID,
        book_order_id: u128,
        test: &mut Scenario,
    ) {
        test.next_tx(@0x1);
        let pool = test.take_shared_by_id<Pool<BaseAsset, QuoteAsset>>(pool_id);
        let (is_bid, _, _,) = utils::decode_order_id(book_order_id);
        borrow_orderbook(&pool, is_bid).borrow(book_order_id);
        return_shared(pool);
    }

    /// Internal function to verifies an order in the book
    fun verify_book_order(
        order: &Order,
        book_order_id: u128,
        client_order_id: u64,
        quantity: u64,
        filled_quantity: u64,
        deep_per_asset: u64,
        epoch: u64,
        status: u8,
        expire_timestamp: u64,
    ) {
        assert!(order.order_id() == book_order_id, constants::e_book_order_mismatch());
        assert!(order.client_order_id() == client_order_id, constants::e_book_order_mismatch());
        assert!(order.quantity() == quantity, constants::e_book_order_mismatch());
        assert!(order.filled_quantity() == filled_quantity, constants::e_book_order_mismatch());
        assert!(order.order_deep_price().deep_per_asset() == deep_per_asset, constants::e_book_order_mismatch());
        assert!(order.epoch() == epoch, constants::e_book_order_mismatch());
        assert!(order.status() == status, constants::e_book_order_mismatch());
        assert!(order.expire_timestamp() == expire_timestamp, constants::e_book_order_mismatch());
    }

    /// Internal function to borrow orderbook
    fun borrow_orderbook<BaseAsset, QuoteAsset>(
        pool: &Pool<BaseAsset, QuoteAsset>,
        is_bid: bool,
    ): &BigVector<Order>{
        let orderbook = if (is_bid) {
            pool.load_inner().bids()
        } else {
            pool.load_inner().asks()
        };
        orderbook
    }

    /// Set the time in the global clock
    fun get_time(
        test: &mut Scenario,
    ): u64 {
        test.next_tx(OWNER);
        {
            let clock = test.take_shared<Clock>();
            let time =clock.timestamp_ms();
            return_shared(clock);

            time
        }
    }

    /// Place swap exact amount order
    fun place_swap_exact_base_for_quote<BaseAsset, QuoteAsset>(
        pool_id: ID,
        trader: address,
        base_in: u64,
        deep_in: u64,
        test: &mut Scenario,
    ): (Coin<BaseAsset>, Coin<QuoteAsset>, Coin<DEEP>) {
        test.next_tx(trader);
        {
            let mut pool = test.take_shared_by_id<Pool<BaseAsset, QuoteAsset>>(pool_id);
            let clock = test.take_shared<Clock>();

            // Place order in pool
            let (base_out, quote_out, deep_out) =
                pool.swap_exact_base_for_quote<BaseAsset, QuoteAsset>(
                    mint_for_testing<BaseAsset>(base_in, test.ctx()),
                    mint_for_testing<DEEP>(deep_in, test.ctx()),
                    &clock,
                    test.ctx()
                );
            return_shared(pool);
            return_shared(clock);

            (base_out, quote_out, deep_out)
        }
    }

    fun place_swap_exact_quote_for_base<BaseAsset, QuoteAsset>(
        pool_id: ID,
        trader: address,
        quote_in: u64,
        deep_in: u64,
        test: &mut Scenario,
    ): (Coin<BaseAsset>, Coin<QuoteAsset>, Coin<DEEP>) {
        test.next_tx(trader);
        {
            let mut pool = test.take_shared_by_id<Pool<BaseAsset, QuoteAsset>>(pool_id);
            let clock = test.take_shared<Clock>();

            // Place order in pool
            let (base_out, quote_out, deep_out) =
                pool.swap_exact_quote_for_base<BaseAsset, QuoteAsset>(
                    mint_for_testing<QuoteAsset>(quote_in, test.ctx()),
                    mint_for_testing<DEEP>(deep_in, test.ctx()),
                    &clock,
                    test.ctx()
                );
            return_shared(pool);
            return_shared(clock);

            (base_out, quote_out, deep_out)
        }
    }

    fun cancel_all_orders<BaseAsset, QuoteAsset>(
        pool_id: ID,
        owner: address,
        balance_manager_id: ID,
        test: &mut Scenario,
    ) {
        test.next_tx(owner);
        {
            let mut pool = test.take_shared_by_id<Pool<BaseAsset, QuoteAsset>>(pool_id);
            let clock = test.take_shared<Clock>();
            let mut balance_manager = test.take_shared_by_id<BalanceManager>(balance_manager_id);
            let trade_proof = balance_manager.generate_proof_as_owner(test.ctx());

            pool.cancel_all_orders<BaseAsset, QuoteAsset>(
                &mut balance_manager,
                &trade_proof,
                &clock,
                test.ctx()
            );
            return_shared(pool);
            return_shared(clock);
            return_shared(balance_manager);
        }
    }

    fun share_clock(
        test: &mut Scenario,
    ) {
        test.next_tx(OWNER);
        clock::create_for_testing(test.ctx()).share_for_testing();
    }

    fun share_registry_for_testing(
        test: &mut Scenario,
    ): ID {
        test.next_tx(OWNER);
        registry::test_registry(test.ctx())
    }

    fun setup_pool<BaseAsset, QuoteAsset>(
        sender: address,
        tick_size: u64,
        lot_size: u64,
        min_size: u64,
        registry_id: ID,
        whitelisted_pool: bool,
        stable_pool: bool,
        creation_fee: Coin<DEEP>,
        test: &mut Scenario,
    ): ID {
        test.next_tx(sender);
        let admin_cap = registry::get_admin_cap_for_testing(test.ctx());
        let mut registry = test.take_shared_by_id<Registry>(registry_id);
        let pool_id;
        {
            pool_id = pool::create_pool_admin<BaseAsset, QuoteAsset>(
                &mut registry,
                tick_size,
                lot_size,
                min_size,
                creation_fee,
                whitelisted_pool,
                stable_pool,
                &admin_cap,
                test.ctx()
            );
        };
        return_shared(registry);
        test_utils::destroy(admin_cap);

        pool_id
    }

    fun get_mid_price<BaseAsset, QuoteAsset>(
        pool_id: ID,
        test: &mut Scenario,
    ): u64 {
        test.next_tx(OWNER);
        {
            let pool = test.take_shared_by_id<Pool<BaseAsset, QuoteAsset>>(pool_id);
            let clock = test.take_shared<Clock>();

            let mid_price = pool.mid_price<BaseAsset, QuoteAsset>(&clock);
            return_shared(pool);
            return_shared(clock);

            mid_price
        }
    }

    fun get_quantity_out<BaseAsset, QuoteAsset>(
        pool_id: ID,
        base_quantity: u64,
        quote_quantity: u64,
        test: &mut Scenario,
    ): (u64, u64, u64) {
        test.next_tx(OWNER);
        {
            let pool = test.take_shared_by_id<Pool<BaseAsset, QuoteAsset>>(pool_id);
            let clock = test.take_shared<Clock>();

            let (base_out, quote_out, deep_required) = pool.get_quantity_out<BaseAsset, QuoteAsset>(
                base_quantity,
                quote_quantity,
                &clock
            );
            return_shared(pool);
            return_shared(clock);

            (base_out, quote_out, deep_required)
        }
    }

    fun get_base_quantity_out<BaseAsset, QuoteAsset>(
        pool_id: ID,
        quote_quantity: u64,
        test: &mut Scenario,
    ): (u64, u64, u64) {
        test.next_tx(OWNER);
        {
            let pool = test.take_shared_by_id<Pool<BaseAsset, QuoteAsset>>(pool_id);
            let clock = test.take_shared<Clock>();

            let (base_out, quote_out, deep_required) = pool.get_base_quantity_out<BaseAsset, QuoteAsset>(
                quote_quantity,
                &clock
            );
            return_shared(pool);
            return_shared(clock);

            (base_out, quote_out, deep_required)
        }
    }

    fun get_quote_quantity_out<BaseAsset, QuoteAsset>(
        pool_id: ID,
        base_quantity: u64,
        test: &mut Scenario,
    ): (u64, u64, u64) {
        test.next_tx(OWNER);
        {
            let pool = test.take_shared_by_id<Pool<BaseAsset, QuoteAsset>>(pool_id);
            let clock = test.take_shared<Clock>();

            let (base_out, quote_out, deep_required) = pool.get_quote_quantity_out<BaseAsset, QuoteAsset>(
                base_quantity,
                &clock
            );
            return_shared(pool);
            return_shared(clock);

            (base_out, quote_out, deep_required)
        }
    }
}

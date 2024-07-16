module deepbook::state_tests {
    use sui::{
        test_scenario::{next_tx, begin, end},
        test_utils::{assert_eq, destroy},
        object::id_from_address,
    };
    use deepbook::{
        utils,
        state::Self,
        balances,
        constants,
        order_info_tests::{create_order_info_base, create_order_info},
    };

    const OWNER: address = @0xF;
    const ALICE: address = @0xA;
    const BOB: address = @0xB;

    #[test]
    fun process_create_ok() {
        let mut test = begin(OWNER);

        test.next_tx(ALICE);
        let taker_price = 1 * constants::usdc_unit();
        let taker_quantity = 10 * constants::sui_unit();
        let mut taker_order = create_order_info_base(BOB, taker_price, taker_quantity, false, test.ctx().epoch());

        let stable_pool = false;
        let mut state = state::empty(stable_pool, test.ctx());
        let price = 1 * constants::usdc_unit();
        let quantity = 1 * constants::sui_unit();
        let mut order_info1 = create_order_info_base(ALICE, price, quantity, true, test.ctx().epoch());
        let (settled, owed) = state.process_create(&mut order_info1, test.ctx());
        assert_eq(settled, balances::new(0, 0, 0));
        assert_eq(owed, balances::new(0, 1 * constants::usdc_unit(), 500_000));
        taker_order.match_maker(&mut order_info1.to_order(), 0);

        test.next_tx(ALICE);
        let price = 1_001_000; // 1.001
        let quantity = 1_001_001_000; // 1.001001
        let mut order_info2 = create_order_info_base(ALICE, price, quantity, true, test.ctx().epoch());
        let (settled, owed) = state.process_create(&mut order_info2, test.ctx());
        assert_eq(settled, balances::new(0, 0, 0));
        assert_eq(owed, balances::new(0, 1_002_002, 500_500)); // rounds down
        taker_order.match_maker(&mut order_info2.to_order(), 0);

        test.next_tx(ALICE);
        let price = 9_999_999_999_000; // $9,999,999.999
        let quantity = 1_999_000_000; // 1.999
        let mut order_info3 = create_order_info_base(ALICE, price, quantity, false, test.ctx().epoch());
        let (settled, owed) = state.process_create(&mut order_info3, test.ctx());
        assert_eq(settled, balances::new(0, 0, 0));
        assert_eq(owed, balances::new(1_999_000_000, 0, 999_500));

        // the taker order has filled the first two maker orders and has some quantities remaining.
        // filled quantity = 1 + 1.001001 = 2.001001
        // quote quantity = 1 * 1 + 1.001001 * 1.001 = 2.002002001 rounds down to 2.002002
        // remaining quantity = 10 - 2.001001 = 7.998999
        // taker gets reduced taker fees (no stake required)
        // taker fees = 2.001001 * 0.001 = 0.002001001
        // maker fees = 7.998999 * 0.0005 = 0.0039994995 rounds down to 0.003999499
        // total fees = 0.002001001 + 0.003999499 = 0.0060005 = 6000500
        let (settled, owed) = state.process_create(&mut taker_order, test.ctx());
        assert_eq(settled, balances::new(0, 2_002_002, 0));
        assert_eq(owed, balances::new(10 * constants::sui_unit(), 0, 6_000_500));

        // Alice has 1 open order remaining. The first two orders have been filled.
        let alice = state.account(id_from_address(ALICE));
        assert!(alice.total_volume() == 2_001_001_000, 0);
        assert!(alice.open_orders().size() == 1, 0);
        assert!(alice.open_orders().contains(&order_info3.order_id()), 0);
        // she traded BOB for 2.001001 SUI
        assert_eq(alice.settled_balances(), balances::new(2_001_001_000, 0, 0));
        assert_eq(alice.owed_balances(), balances::new(0, 0, 0));

        // Bob has 1 open order after the partial fill.
        let bob = state.account(id_from_address(BOB));
        assert!(bob.total_volume() == 2_001_001_000, 0);
        assert!(bob.open_orders().size() == 1, 0);
        assert!(bob.open_orders().contains(&taker_order.order_id()), 0);
        // Bob's balances have been settled already
        assert_eq(bob.settled_balances(), balances::new(0, 0, 0));
        assert_eq(bob.owed_balances(), balances::new(0, 0, 0));

        destroy(state);
        test.end();
    }

    #[test]
    // BOB sells 10 SUI at $1 with deep_per_base of 21
    // gets matched with ALICE who has 13 buys at $13
    fun process_create_deep_price_ok() {
        let mut test = begin(OWNER);

        test.next_tx(ALICE);
        let taker_price = 1 * constants::usdc_unit();
        let taker_quantity = 10 * constants::sui_unit();
        let balance_manager_id = id_from_address(BOB);
        let order_type = 0;
        let fee_is_deep = true;
        let deep_per_asset = 21 * constants::float_scaling();
        let market_order = false;
        let expire_timestamp = constants::max_u64();
        let conversion_is_base = true;
        let mut taker_order = create_order_info(
            balance_manager_id,
            BOB,
            order_type,
            taker_price,
            taker_quantity,
            false,
            fee_is_deep,
            test.ctx().epoch(),
            expire_timestamp,
            deep_per_asset,
            conversion_is_base,
            market_order
        );

        let stable_pool = false;
        let mut state = state::empty(stable_pool, test.ctx());
        let price = 13 * constants::usdc_unit();
        let quantity = 13 * constants::sui_unit();
        let mut order_info = create_order_info_base(ALICE, price, quantity, true, test.ctx().epoch());
        let (settled, owed) = state.process_create(&mut order_info, test.ctx());
        assert_eq(settled, balances::new(0, 0, 0));
        assert_eq(owed, balances::new(0, 169 * constants::usdc_unit(), 6_500_000));

        taker_order.match_maker(&mut order_info.to_order(), 0);
        let (settled, owed) = state.process_create(&mut taker_order, test.ctx());

        assert_eq(settled, balances::new(0, 130 * constants::usdc_unit(), 0));
        // taker fee 0.001, quantity 10, deep_per_base 21
        // 10 * 21 * 0.001 = 0.21 = 210000000
        assert_eq(owed, balances::new(10_000_000_000, 0, 210_000_000));

        destroy(state);
        test.end();
    }

    #[test]
    // process create with maker in epoch 0, then gov to change fees, then taker in epoch 1
    fun process_create_stake_req_ok() {
        let mut test = begin(OWNER);

        test.next_tx(ALICE);
        let stable_pool = false;
        let mut state = state::empty(stable_pool, test.ctx());
        state.process_stake(id_from_address(ALICE), 100 * constants::sui_unit(), test.ctx());

        test.next_epoch(OWNER);
        test.next_tx(ALICE);
        // change fee structure
        state.process_proposal(id_from_address(ALICE), 500000, 200000, 100 * constants::sui_unit(), test.ctx());

        // place maker with old fee structure
        test.next_tx(ALICE);
        let price = 1 * constants::usdc_unit();
        let quantity = 10 * constants::sui_unit();
        let mut order_info = create_order_info_base(ALICE, price, quantity, true, test.ctx().epoch());
        state.process_create(&mut order_info, test.ctx());

        // place taker with new fee structure
        test.next_epoch(OWNER);
        test.next_tx(ALICE);
        let taker_price = 1 * constants::usdc_unit();
        let taker_quantity = 1 * constants::sui_unit();
        let mut taker_order = create_order_info_base(BOB, taker_price, taker_quantity, false, test.ctx().epoch());
        taker_order.match_maker(&mut order_info.to_order(), 0);
        let (settled, owed) = state.process_create(&mut taker_order, test.ctx());
        assert_eq(settled, balances::new(0, 1 * constants::usdc_unit(), 0));
        assert_eq(owed, balances::new(1 * constants::sui_unit(), 0, 500_000));

        destroy(state);
        test.end();
    }

    // process create after governance to raise stake required. taker fee 0.001
    #[test]
    fun process_create_after_raising_steak_req_ok() {
        let mut test = begin(OWNER);
        test.next_tx(ALICE);
        // alice and bob stake 100 DEEP each
        // default stake required is 100
        let stable_pool = false;
        let mut state = state::empty(stable_pool, test.ctx());
        state.process_stake(id_from_address(ALICE), 120 * constants::sui_unit(), test.ctx());
        state.process_stake(id_from_address(BOB), 100 * constants::sui_unit(), test.ctx());

        // to make stakes active
        test.next_epoch(OWNER);

        // still in the current epoch, bob generates 100 volume then 100 volume again. His second order is exercised with lower taker fees.
        test.next_tx(ALICE);
        let price = 1 * constants::usdc_unit();
        let quantity = 1000 * constants::sui_unit();
        let mut order_info = create_order_info_base(ALICE, price, quantity, true, test.ctx().epoch());
        state.process_create(&mut order_info, test.ctx());
        let mut order = order_info.to_order();

        test.next_tx(BOB);
        let taker_quantity = 100 * constants::sui_unit();
        let mut taker_order = create_order_info_base(BOB, price, taker_quantity, false, test.ctx().epoch());
        taker_order.match_maker(&mut order, 0);
        let (settled, owed) = state.process_create(&mut taker_order, test.ctx());
        // bob's first order
        // pays 1 SUI for the trade along with 0.001 DEEP in fees to receive 1 USDC
        assert_eq(settled, balances::new(0, 100 * constants::usdc_unit(), 0));
        assert_eq(owed, balances::new(100 * constants::sui_unit(), 0, 100_000_000));

        // bob's second order, gets reduced taker fees
        test.next_tx(BOB);
        let taker_quantity = 100 * constants::sui_unit();
        let mut taker_order = create_order_info_base(BOB, price, taker_quantity, false, test.ctx().epoch());
        taker_order.set_order_id(utils::encode_order_id(false, price, 2));
        taker_order.match_maker(&mut order, 0);
        let (settled, owed) = state.process_create(&mut taker_order, test.ctx());
        assert_eq(settled, balances::new(0, 100 * constants::usdc_unit(), 0));
        assert_eq(owed, balances::new(100 * constants::sui_unit(), 0, 50_000_000));

        // alice makes a proposal to raise the stake required to 200 and votes for it
        test.next_tx(ALICE);
        state.process_proposal(id_from_address(ALICE), 1000000, 500000, 200 * constants::sui_unit(), test.ctx());

        // new proposal is active, bob can no longer get reduced fees after trading 200 volume
        test.next_epoch(OWNER);

        test.next_tx(BOB);
        let taker_quantity = 200 * constants::sui_unit();
        let mut taker_order = create_order_info_base(BOB, price, taker_quantity, false, test.ctx().epoch());
        taker_order.set_order_id(utils::encode_order_id(false, price, 3));
        taker_order.match_maker(&mut order, 0);
        let (settled, owed) = state.process_create(&mut taker_order, test.ctx());
        assert_eq(settled, balances::new(0, 200 * constants::usdc_unit(), 0));
        assert_eq(owed, balances::new(200 * constants::sui_unit(), 0, 200_000_000));

        // even though bob has 200 volume, since he doesn't have 200 stake, he doesn't get reduced fees
        test.next_tx(BOB);
        let taker_quantity = 200 * constants::sui_unit();
        let mut taker_order = create_order_info_base(BOB, price, taker_quantity, false, test.ctx().epoch());
        taker_order.set_order_id(utils::encode_order_id(false, price, 4));
        taker_order.match_maker(&mut order, 0);
        let (settled, owed) = state.process_create(&mut taker_order, test.ctx());
        assert_eq(settled, balances::new(0, 200 * constants::usdc_unit(), 0));
        assert_eq(owed, balances::new(200 * constants::sui_unit(), 0, 200_000_000));

        destroy(state);
        test.end();
    }

    // process create after gov, then after stake to meet req. taker fee 0.0005
    #[test]
    fun process_create_after_lowering_steak_req_ok() {
        let mut test = begin(OWNER);

        test.next_tx(ALICE);
        // alice and bob stake 50 DEEP each
        // default stake required is 100
        let stable_pool = false;
        let mut state = state::empty(stable_pool, test.ctx());
        state.process_stake(id_from_address(ALICE), 60 * constants::deep_unit(), test.ctx());
        state.process_stake(id_from_address(BOB), 50 * constants::deep_unit(), test.ctx());

        // to make stakes active
        test.next_epoch(OWNER);

        // bob generates 50 volume three times, his fees are not reduced.
        test.next_tx(ALICE);
        let price = 1 * constants::usdc_unit();
        let quantity = 1000 * constants::sui_unit();
        let mut order_info = create_order_info_base(ALICE, price, quantity, true, test.ctx().epoch());
        state.process_create(&mut order_info, test.ctx());
        let mut order = order_info.to_order();

        test.next_tx(BOB);
        let taker_quantity = 50 * constants::sui_unit();
        let mut taker_order = create_order_info_base(BOB, price, taker_quantity, false, test.ctx().epoch());
        taker_order.match_maker(&mut order, 0);
        let (settled, owed) = state.process_create(&mut taker_order, test.ctx());
        // bob's first order
        // pays 1 SUI for the trade along with 0.001 DEEP in fees to receive 1 USDC
        assert_eq(settled, balances::new(0, 50 * constants::usdc_unit(), 0));
        assert_eq(owed, balances::new(50 * constants::sui_unit(), 0, 50_000_000));

        // bob's second order, still no reduced fees
        test.next_tx(BOB);
        let taker_quantity = 50 * constants::sui_unit();
        let mut taker_order = create_order_info_base(BOB, price, taker_quantity, false, test.ctx().epoch());
        taker_order.set_order_id(utils::encode_order_id(false, price, 2));
        taker_order.match_maker(&mut order, 0);
        let (settled, owed) = state.process_create(&mut taker_order, test.ctx());
        assert_eq(settled, balances::new(0, 50 * constants::usdc_unit(), 0));
        assert_eq(owed, balances::new(50 * constants::sui_unit(), 0, 50_000_000));

        // bob's third order, still no reduced fees
        test.next_tx(BOB);
        let taker_quantity = 50 * constants::sui_unit();
        let mut taker_order = create_order_info_base(BOB, price, taker_quantity, false, test.ctx().epoch());
        taker_order.set_order_id(utils::encode_order_id(false, price, 3));
        taker_order.match_maker(&mut order, 0);
        let (settled, owed) = state.process_create(&mut taker_order, test.ctx());
        assert_eq(settled, balances::new(0, 50 * constants::usdc_unit(), 0));
        assert_eq(owed, balances::new(50 * constants::sui_unit(), 0, 50_000_000));

        // alice makes a proposal to lower the stake required to 50 and votes for it
        test.next_tx(ALICE);
        state.process_proposal(id_from_address(ALICE), 1000000, 500000, 50 * constants::deep_unit(), test.ctx());

        // new proposal is active, bob can no longer get reduced fees after trading 200 volume
        test.next_epoch(OWNER);

        test.next_tx(BOB);
        let taker_quantity = 50 * constants::sui_unit();
        let mut taker_order = create_order_info_base(BOB, price, taker_quantity, false, test.ctx().epoch());
        taker_order.set_order_id(utils::encode_order_id(false, price, 4));
        taker_order.match_maker(&mut order, 0);
        let (settled, owed) = state.process_create(&mut taker_order, test.ctx());
        assert_eq(settled, balances::new(0, 50 * constants::usdc_unit(), 0));
        assert_eq(owed, balances::new(50 * constants::sui_unit(), 0, 50_000_000));

        // bob is now over 50 volume and has the necessary stake, his taker fee is reduced
        test.next_tx(BOB);
        let taker_quantity = 50 * constants::sui_unit();
        let mut taker_order = create_order_info_base(BOB, price, taker_quantity, false, test.ctx().epoch());
        taker_order.set_order_id(utils::encode_order_id(false, price, 5));
        taker_order.match_maker(&mut order, 0);
        let (settled, owed) = state.process_create(&mut taker_order, test.ctx());
        assert_eq(settled, balances::new(0, 50 * constants::usdc_unit(), 0));
        assert_eq(owed, balances::new(50 * constants::sui_unit(), 0, 25_000_000));

        destroy(state);
        test.end();
    }

    #[test]
    fun process_cancel_ok() {
        let mut test = begin(OWNER);

        test.next_tx(ALICE);
        let price = 10 * constants::usdc_unit();
        let quantity = 10 * constants::sui_unit();
        let mut order_info = create_order_info_base(ALICE, price, quantity, true, test.ctx().epoch());
        let stable_pool = false;
        let mut state = state::empty(stable_pool, test.ctx());
        let (settled, owed) = state.process_create(&mut order_info, test.ctx());

        assert_eq(settled, balances::new(0, 0, 0));
        // 10 * 10 = 100
        // 10 * 0.0005 = 0.005
        assert_eq(owed, balances::new(0, 100 * constants::usdc_unit(), 5_000_000));

        let (settled, owed) = state.process_cancel(&mut order_info.to_order(), id_from_address(ALICE), test.ctx());
        assert_eq(settled, balances::new(0, 100 * constants::usdc_unit(), 5_000_000));
        assert_eq(owed, balances::new(0, 0, 0));

        destroy(state);
        test.end();
    }

    // process cancel after partial fill
    #[test]
    fun process_cancel_after_partial_ok() {
        let mut test = begin(OWNER);

        test.next_tx(ALICE);
        let price = 10 * constants::usdc_unit();
        let quantity = 10 * constants::sui_unit();
        let mut order_info = create_order_info_base(ALICE, price, quantity, true, test.ctx().epoch());
        let stable_pool = false;
        let mut state = state::empty(stable_pool, test.ctx());
        state.process_create(&mut order_info, test.ctx());

        test.next_tx(ALICE);
        let price = 1 * constants::usdc_unit();
        let quantity = 1 * constants::sui_unit();
        let mut taker_order = create_order_info_base(BOB, price, quantity, false, test.ctx().epoch());
        let mut order = order_info.to_order();
        taker_order.match_maker(&mut order, 0);
        state.process_create(&mut taker_order, test.ctx());

        test.next_tx(ALICE);
        let (settled, owed) = state.process_cancel(&mut order, id_from_address(ALICE), test.ctx());
        // paid 100 USDC to buy 10 SUI. 1 SUI filled.
        // returns 90 USDC and 1 SUI, along with 4_500_000 in DEEP
        assert_eq(settled, balances::new(1 * constants::sui_unit(), 90 * constants::usdc_unit(), 4_500_000));
        assert_eq(owed, balances::new(0, 0, 0));

        destroy(state);
        test.end();
    }

    // process cancel after modify after epoch change & maker fee change
    #[test]
    fun process_canecel_after_modify_epoch_change_ok() {
        let mut test = begin(OWNER);

        test.next_tx(ALICE);
        // stake 100 DEEP
        let stable_pool = false;
        let mut state = state::empty(stable_pool, test.ctx());
        state.process_stake(id_from_address(ALICE), 100 * constants::sui_unit(), test.ctx());

        // place maker order
        let price = 10 * constants::usdc_unit();
        let quantity = 10 * constants::sui_unit();
        let mut order_info = create_order_info_base(ALICE, price, quantity, true, test.ctx().epoch());
        state.process_create(&mut order_info, test.ctx());

        test.next_epoch(OWNER);
        test.next_tx(ALICE);
        // propose to reduce fees
        state.process_proposal(id_from_address(ALICE), 500000, 200000, 100 * constants::sui_unit(), test.ctx());

        test.next_epoch(OWNER);
        test.next_tx(ALICE);
        // modify maker order
        let mut order = order_info.to_order();
        let cancel_quantity = 5 * constants::sui_unit();
        order.modify(cancel_quantity, constants::max_u64() - 1);
        let (settled, owed) = state.process_modify(id_from_address(ALICE), 5 * constants::sui_unit(), &order, test.ctx());
        // reduces quantity from 10 to 5. Get refund of 50 USDC and half of the fees
        assert_eq(settled, balances::new(0, 50 * constants::usdc_unit(), 2_500_000));
        assert_eq(owed, balances::new(0, 0, 0));

        test.next_tx(ALICE);
        // regardless of the fee change, when canceling the remaining amount, get refund of 50 USDC and rest of the fees (other half)
        let (settled, owed) = state.process_cancel(&mut order, id_from_address(ALICE), test.ctx());
        assert_eq(settled, balances::new(0, 50 * constants::usdc_unit(), 2_500_000));
        assert_eq(owed, balances::new(0, 0, 0));

        destroy(state);
        test.end();
    }

    // process stake
    #[test]
    fun process_stake_ok() {
        let mut test = begin(OWNER);

        test.next_tx(ALICE);
        let stable_pool = false;
        let mut state = state::empty(stable_pool, test.ctx());
        let (settled, owed) = state.process_stake(id_from_address(ALICE), 1 * constants::sui_unit(), test.ctx());
        assert_eq(settled, balances::new(0, 0, 0));
        assert_eq(owed, balances::new(0, 0, 1 * constants::sui_unit()));
        assert!(state.governance().voting_power() == 1_000_000_000, 0);
        state.process_stake(id_from_address(BOB), 1 * constants::sui_unit(), test.ctx());
        assert!(state.governance().voting_power() == 2_000_000_000, 0);

        let (settled, owed) = state.process_unstake(id_from_address(ALICE), test.ctx());
        assert_eq(settled, balances::new(0, 0, 1 * constants::sui_unit()));
        assert_eq(owed, balances::new(0, 0, 0));
        assert!(state.governance().voting_power() == 1_000_000_000, 0);
        let (settled, owed) = state.process_unstake(id_from_address(BOB), test.ctx());
        assert_eq(settled, balances::new(0, 0, 1 * constants::sui_unit()));
        assert_eq(owed, balances::new(0, 0, 0));
        assert!(state.governance().voting_power() == 0, 0);

        destroy(state);
        test.end();
    }

    // process proposal
    #[test, expected_failure(abort_code = state::ENoStake)]
    fun process_proposal_no_stake_e() {
        let mut test = begin(OWNER);

        test.next_tx(ALICE);
        let stable_pool = false;
        let mut state = state::empty(stable_pool, test.ctx());
        state.process_proposal(id_from_address(ALICE), 1, 1, 1, test.ctx());

        abort(0)
    }

    #[test, expected_failure(abort_code = state::ENoStake)]
    // have to wait for epoch to turn
    fun process_proposal_no_stake_e2() {
        let mut test = begin(OWNER);

        test.next_tx(ALICE);
        let stable_pool = false;
        let mut state = state::empty(stable_pool, test.ctx());
        state.process_stake(id_from_address(ALICE), 1 * constants::sui_unit(), test.ctx());
        state.process_proposal(id_from_address(ALICE), 1, 1, 1, test.ctx());

        abort(0)
    }

    #[test]
    fun process_proposal_vote_ok() {
        let mut test = begin(OWNER);

        test.next_tx(ALICE);
        let stable_pool = false;
        let mut state = state::empty(stable_pool, test.ctx());
        state.process_stake(id_from_address(ALICE), 100 * constants::deep_unit(), test.ctx());
        state.process_stake(id_from_address(BOB), 250 * constants::deep_unit(), test.ctx());

        test.next_epoch(OWNER);
        test.next_tx(ALICE);
        state.process_proposal(id_from_address(ALICE), 500000, 200000, 100 * constants::deep_unit(), test.ctx());
        // total voting power = 50 + (sqrt(100) - sqrt(50)) = 50 + 10 - 7.071067811 = 52.928932189 rounded down
        // total voting power = 50 + (sqrt(250) - sqrt(50)) = 50 + 15.811388300 - 7.071067811 = 58.740320489 rounded down
        // total = 52.928932189 + 58.740320489 = 111.669252678
        // quorum = 111.669252678 * 0.5 = 55.834626339 rouned down
        assert!(state.governance().voting_power() == 350 * constants::deep_unit(), 0);
        assert!(state.governance().quorum() == 175 * constants::deep_unit(), 0);
        assert!(state.governance().proposals().get(&id_from_address(ALICE)).votes() == 100 * constants::deep_unit(), 0);

        // bob votes on alice's proposal
        state.process_vote(id_from_address(BOB), id_from_address(ALICE), test.ctx());
        assert!(state.governance().proposals().get(&id_from_address(ALICE)).votes() == 350 * constants::deep_unit(), 0);

        // alice unstakes, removing her vote
        state.process_unstake(id_from_address(ALICE), test.ctx());
        assert!(state.governance().voting_power() == 250 * constants::deep_unit(), 0);
        assert!(state.governance().proposals().get(&id_from_address(ALICE)).votes() == 250 * constants::deep_unit(), 0);

        // proposal still goes through since 250 >= 175
        test.next_epoch(OWNER);
        state.process_proposal(id_from_address(BOB), 600000, 300000, 200 * constants::deep_unit(), test.ctx());
        assert!(state.governance().trade_params().maker_fee() == 200000, 0);
        assert!(state.governance().trade_params().taker_fee() == 500000, 0);
        assert!(state.governance().trade_params().stake_required() == 100 * constants::deep_unit(), 0);

        destroy(state);
        test.end();
    }
}

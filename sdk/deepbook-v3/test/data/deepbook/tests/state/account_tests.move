// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module deepbook::account_tests {
    use sui::{
        test_scenario::{next_tx, begin, end},
        test_utils::assert_eq,
        object::id_from_address,
    };
    use deepbook::{
        account,
        balances,
        fill,
        constants,
        deep_price
    };

    const OWNER: address = @0xF;
    const ALICE: address = @0xA;

    #[test]
    fun add_balances_ok() {
        let mut test = begin(OWNER);

        test.next_tx(ALICE);
        let mut account = account::empty(test.ctx());
        let (settled, owed) = account.settle();
        assert_eq(settled, balances::new(0, 0, 0));
        assert_eq(owed, balances::new(0, 0, 0));

        account.add_settled_balances(balances::new(1, 2, 3));
        account.add_owed_balances(balances::new(4, 5, 6));
        let (settled, owed) = account.settle();
        assert_eq(settled, balances::new(1, 2, 3));
        assert_eq(owed, balances::new(4, 5, 6));

        test.end();
    }

    #[test]
    fun process_maker_fill_ok() {
        let mut test = begin(OWNER);

        test.next_tx(ALICE);
        let mut account = account::empty(test.ctx());
        account.add_order(1);
        let fill = fill::new(1, 1, 1, id_from_address(@0xB), false, false, 100, 500, false, 0, deep_price::new_order_deep_price(true, constants::deep_multiplier()));
        account.process_maker_fill(&fill);
        let (settled, owed) = account.settle();
        assert_eq(settled, balances::new(100, 0, 0));
        assert_eq(owed, balances::new(0, 0, 0));
        assert!(account.total_volume() == 100, 0);
        assert!(account.open_orders().size() == 1, 0);
        assert!(account.open_orders().contains(&(1 as u128)), 0);

        account.add_order(2);
        let fill = fill::new(2, 2, 1, id_from_address(@0xC), false, true, 100, 500, true, 0, deep_price::new_order_deep_price(true, constants::deep_multiplier()));
        account.process_maker_fill(&fill);
        let (settled, owed) = account.settle();
        assert_eq(settled, balances::new(0, 500, 0));
        assert_eq(owed, balances::new(0, 0, 0));
        assert!(account.total_volume() == 200, 0);
        assert!(account.open_orders().size() == 1, 0);
        assert!(account.open_orders().contains(&(1 as u128)), 0);
        assert!(!account.open_orders().contains(&(2 as u128)), 0);

        account.add_order(3);
        let fill = fill::new(3, 3, 1, id_from_address(@0xC), true, false, 100, 500, true, 0, deep_price::new_order_deep_price(true, constants::deep_multiplier()));
        account.process_maker_fill(&fill);
        let (settled, owed) = account.settle();
        assert_eq(settled, balances::new(100, 0, 0));
        assert_eq(owed, balances::new(0, 0, 0));
        assert!(account.total_volume() == 200, 0);
        assert!(account.open_orders().size() == 1, 0);
        assert!(account.open_orders().contains(&(1 as u128)), 0);
        assert!(!account.open_orders().contains(&(2 as u128)), 0);
        assert!(!account.open_orders().contains(&(3 as u128)), 0);

        account.add_order(4);
        let fill = fill::new(4, 4, 1, id_from_address(@0xC), false, true, 100, 500, true, 0, deep_price::new_order_deep_price(true, constants::deep_multiplier()));
        account.process_maker_fill(&fill);
        let (settled, owed) = account.settle();
        assert_eq(settled, balances::new(0, 500, 0));
        assert_eq(owed, balances::new(0, 0, 0));
        assert!(account.total_volume() == 300, 0);
        assert!(account.open_orders().size() == 1, 0);
        assert!(account.open_orders().contains(&(1 as u128)), 0);
        assert!(!account.open_orders().contains(&(2 as u128)), 0);
        assert!(!account.open_orders().contains(&(3 as u128)), 0);
        assert!(!account.open_orders().contains(&(4 as u128)), 0);

        test.end();
    }

    #[test]
    fun add_remove_stake_ok() {
        let mut test = begin(OWNER);

        test.next_tx(ALICE);
        let mut account = account::empty(test.ctx());
        let (before, after) = account.add_stake(100);
        assert!(before == 0, 0);
        assert!(after == 100, 0);
        assert!(account.active_stake() == 0, 0);
        assert!(account.inactive_stake() == 100, 0);

        let (before, after) = account.add_stake(100);
        assert!(before == 100, 0);
        assert!(after == 200, 0);
        assert!(account.active_stake() == 0, 0);
        assert!(account.inactive_stake() == 200, 0);
        let (settled, owed) = account.settle();
        assert_eq(settled, balances::new(0, 0, 0));
        assert_eq(owed, balances::new(0, 0, 200));

        account.remove_stake();
        assert!(account.active_stake() == 0, 0);
        assert!(account.inactive_stake() == 0, 0);
        let (settled, owed) = account.settle();
        assert_eq(settled, balances::new(0, 0, 200));
        assert_eq(owed, balances::new(0, 0, 0));

        let (before, after) = account.add_stake(0);
        assert!(before == 0, 0);
        assert!(after == 0, 0);
        assert!(account.active_stake() == 0, 0);
        assert!(account.inactive_stake() == 0, 0);
        let (settled, owed) = account.settle();
        assert_eq(settled, balances::new(0, 0, 0));
        assert_eq(owed, balances::new(0, 0, 0));

        test.end();
    }

    #[test]
    fun update_ok() {
        let mut test = begin(OWNER);

        test.next_tx(ALICE);
        let mut account = account::empty(test.ctx());
        let (prev_epoch, prev_maker_volume, prev_active_stake) = account.update(test.ctx());
        assert!(prev_epoch == 0, 0);
        assert!(prev_maker_volume == 0, 0);
        assert!(prev_active_stake == 0, 0);

        account.add_order(1);
        let fill = fill::new(1, 1, 1, id_from_address(@0xB), false, false, 100, 500, false, 0, deep_price::new_order_deep_price(true, constants::deep_multiplier()));
        account.process_maker_fill(&fill);

        // update doesn't do anything until next epoch
        let (prev_epoch, prev_maker_volume, prev_active_stake) = account.update(test.ctx());
        assert!(prev_epoch == 0, 0);
        assert!(prev_maker_volume == 0, 0);
        assert!(prev_active_stake == 0, 0);

        test.next_epoch(OWNER);
        test.next_tx(ALICE);
        let (prev_epoch, prev_maker_volume, prev_active_stake) = account.update(test.ctx());
        assert!(prev_epoch == 0, 0);
        assert!(prev_maker_volume == 100, 0);
        assert!(prev_active_stake == 0, 0);

        let (before, after) = account.add_stake(100);
        assert!(before == 0, 0);
        assert!(after == 100, 0);
        assert!(account.active_stake() == 0, 0);
        assert!(account.inactive_stake() == 100, 0);

        // already reset earlier, new stake not counted yet
        let (prev_epoch, prev_maker_volume, prev_active_stake) = account.update(test.ctx());
        assert!(prev_epoch == 0, 0);
        assert!(prev_maker_volume == 0, 0);
        assert!(prev_active_stake == 0, 0);

        test.next_epoch(OWNER);
        test.next_tx(ALICE);
        let (prev_epoch, prev_maker_volume, prev_active_stake) = account.update(test.ctx());
        assert!(prev_epoch == 1, 0);
        assert!(prev_maker_volume == 0, 0);
        assert!(prev_active_stake == 0, 0);
        // prev active stake still zero, but current active stake updated
        assert!(account.active_stake() == 100, 0);
        assert!(account.inactive_stake() == 0, 0);

        let (before, after) = account.add_stake(100);
        assert!(before == 100, 0);
        assert!(after == 200, 0);

        test.next_epoch(OWNER);
        test.next_tx(ALICE);
        let (prev_epoch, prev_maker_volume, prev_active_stake) = account.update(test.ctx());
        assert!(prev_epoch == 2, 0);
        assert!(prev_maker_volume == 0, 0);
        assert!(prev_active_stake == 100, 0);
        assert!(account.active_stake() == 200, 0);
        assert!(account.inactive_stake() == 0, 0);

        test.end();
    }

    #[test]
    fun claim_rebates_ok() {
        let mut test = begin(OWNER);

        test.next_tx(ALICE);
        let mut account = account::empty(test.ctx());
        account.claim_rebates();
        let (settled, owed) = account.settle();
        assert_eq(settled, balances::new(0, 0, 0));
        assert_eq(owed, balances::new(0, 0, 0));

        account.add_rebates(balances::new(0, 0, 100));
        account.claim_rebates();
        let (settled, owed) = account.settle();
        assert_eq(settled, balances::new(0, 0, 100));
        assert_eq(owed, balances::new(0, 0, 0));

        // user owes 100 DEEP for staking
        account.add_stake(100);
        // user receives 100 DEEP from rebates
        account.add_rebates(balances::new(0, 0, 100));
        account.claim_rebates();
        let (settled, owed) = account.settle();
        assert_eq(settled, balances::new(0, 0, 100));
        assert_eq(owed, balances::new(0, 0, 100));

        test.end();
    }

    #[test]
    fun set_voted_proposal_ok() {
        let mut test = begin(OWNER);

        test.next_tx(ALICE);
        let mut account = account::empty(test.ctx());
        assert!(account.voted_proposal().is_none(), 0);

        let prev_proposal = account.set_voted_proposal(option::some(id_from_address(@0x1)));
        assert!(prev_proposal.is_none(), 0);
        assert!(account.voted_proposal().borrow() == id_from_address(@0x1), 0);

        let prev_proposal = account.set_voted_proposal(option::some(id_from_address(@0x2)));
        assert!(prev_proposal.borrow() == id_from_address(@0x1), 0);
        assert!(account.voted_proposal().borrow() == id_from_address(@0x2), 0);

        let prev_proposal = account.set_voted_proposal(option::none());
        assert!(prev_proposal.borrow() == id_from_address(@0x2), 0);
        assert!(account.voted_proposal().is_none(), 0);

        test.end();
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module deepbook::vault_tests {
    use sui::{
        test_scenario::{next_tx, begin, end},
        test_utils::destroy,
        object::id_from_address,
    };
    use deepbook::{
        vault::Self,
        balance_manager_tests::{USDC, SPAM, create_acct_and_share_with_funds},
        constants,
        balances::Self,
        balance_manager::BalanceManager,
    };

    const OWNER: address = @0xF;
    const ALICE: address = @0xA;

    #[test]
    fun borrow_flashloan_ok() {
        let mut test = begin(OWNER);

        let balance_manager_id = create_acct_and_share_with_funds(ALICE, 1000000 * constants::float_scaling(), &mut test);
        test.next_tx(ALICE);
        let mut vault = vault::empty<SPAM, USDC>();
        let settled_balances = balances::new(0, 0, 0);
        let owed_balances = balances::new(1000, 1000, 1000);
        let mut balance_manager = test.take_shared_by_id<BalanceManager>(balance_manager_id);
        let trade_proof = balance_manager.generate_proof_as_owner(test.ctx());

        // move funds into the vault
        vault.settle_balance_manager(settled_balances, owed_balances, &mut balance_manager, &trade_proof);

        // borrow flashloan
        let (base, base_loan) = vault.borrow_flashloan_base(id_from_address(@0x1), 1000, test.ctx());
        let (quote, quote_loan) = vault.borrow_flashloan_quote(id_from_address(@0x1), 1000, test.ctx());
        vault.return_flashloan_base(id_from_address(@0x1), base, base_loan);
        vault.return_flashloan_quote(id_from_address(@0x1), quote, quote_loan);

        destroy(vault);
        destroy(balance_manager);
        test.end();
    }

    #[test]
    fun borrow_flashloan_single_ok() {
        let mut test = begin(OWNER);

        let balance_manager_id = create_acct_and_share_with_funds(ALICE, 1000000 * constants::float_scaling(), &mut test);
        test.next_tx(ALICE);
        let mut vault = vault::empty<SPAM, USDC>();
        let settled_balances = balances::new(0, 0, 0);
        let owed_balances = balances::new(1000, 1000, 1000);
        let mut balance_manager = test.take_shared_by_id<BalanceManager>(balance_manager_id);
        let trade_proof = balance_manager.generate_proof_as_owner(test.ctx());

        // move funds into the vault
        vault.settle_balance_manager(settled_balances, owed_balances, &mut balance_manager, &trade_proof);

        // borrow flashloan
        let (quote, loan) = vault.borrow_flashloan_quote(id_from_address(@0x1), 1000, test.ctx());
        vault.return_flashloan_quote(id_from_address(@0x1), quote, loan);

        destroy(vault);
        destroy(balance_manager);
        test.end();
    }

    #[test, expected_failure(abort_code = vault::ENotEnoughBaseForLoan)]
    fun borrow_flashloan_not_enough_base_e() {
        let mut test = begin(OWNER);

        let balance_manager_id = create_acct_and_share_with_funds(ALICE, 1000000 * constants::float_scaling(), &mut test);
        test.next_tx(ALICE);
        let mut vault = vault::empty<SPAM, USDC>();
        let settled_balances = balances::new(0, 0, 0);
        let owed_balances = balances::new(1000, 1000, 1000);
        let mut balance_manager = test.take_shared_by_id<BalanceManager>(balance_manager_id);
        let trade_proof = balance_manager.generate_proof_as_owner(test.ctx());

        // move funds into the vault
        vault.settle_balance_manager(settled_balances, owed_balances, &mut balance_manager, &trade_proof);

        // borrow flashloan
        let (_base, _loan) = vault.borrow_flashloan_base(id_from_address(@0x1), 1001, test.ctx());
        let (_quote, _loan) = vault.borrow_flashloan_quote(id_from_address(@0x1), 1000, test.ctx());

        abort(0)
    }

    #[test, expected_failure(abort_code = vault::ENotEnoughQuoteForLoan)]
    fun borrow_flashloan_not_enough_quote_e() {
        let mut test = begin(OWNER);

        let balance_manager_id = create_acct_and_share_with_funds(ALICE, 1000000 * constants::float_scaling(), &mut test);
        test.next_tx(ALICE);
        let mut vault = vault::empty<SPAM, USDC>();
        let settled_balances = balances::new(0, 0, 0);
        let owed_balances = balances::new(1000, 1000, 1000);
        let mut balance_manager = test.take_shared_by_id<BalanceManager>(balance_manager_id);
        let trade_proof = balance_manager.generate_proof_as_owner(test.ctx());

        // move funds into the vault
        vault.settle_balance_manager(settled_balances, owed_balances, &mut balance_manager, &trade_proof);

        // borrow flashloan
        let (_base, _loan) = vault.borrow_flashloan_base(id_from_address(@0x1), 1000, test.ctx());
        let (_quote, _loan) = vault.borrow_flashloan_quote(id_from_address(@0x1), 1001, test.ctx());

        abort 0
    }

    #[test, expected_failure(abort_code = vault::EIncorrectLoanPool)]
    fun borrow_flashloan_incorrect_pool_id_e() {
        let mut test = begin(OWNER);

        let balance_manager_id = create_acct_and_share_with_funds(ALICE, 1000000 * constants::float_scaling(), &mut test);
        test.next_tx(ALICE);
        let mut vault = vault::empty<SPAM, USDC>();
        let settled_balances = balances::new(0, 0, 0);
        let owed_balances = balances::new(1000, 1000, 1000);
        let mut balance_manager = test.take_shared_by_id<BalanceManager>(balance_manager_id);
        let trade_proof = balance_manager.generate_proof_as_owner(test.ctx());

        // move funds into the vault
        vault.settle_balance_manager(settled_balances, owed_balances, &mut balance_manager, &trade_proof);

        // borrow flashloan
        let (base, base_loan) = vault.borrow_flashloan_base(id_from_address(@0x1), 1000, test.ctx());
        vault.return_flashloan_base(id_from_address(@0x2), base, base_loan);

        abort(0)
    }

    #[test, expected_failure(abort_code = vault::EIncorrectQuantityReturned)]
    fun borrow_flashloan_incorrect_return_base_e() {
        let mut test = begin(OWNER);

        let balance_manager_id = create_acct_and_share_with_funds(ALICE, 1000000 * constants::float_scaling(), &mut test);
        test.next_tx(ALICE);
        let mut vault = vault::empty<SPAM, USDC>();
        let settled_balances = balances::new(0, 0, 0);
        let owed_balances = balances::new(1000, 1000, 1000);
        let mut balance_manager = test.take_shared_by_id<BalanceManager>(balance_manager_id);
        let trade_proof = balance_manager.generate_proof_as_owner(test.ctx());

        // move funds into the vault
        vault.settle_balance_manager(settled_balances, owed_balances, &mut balance_manager, &trade_proof);

        // borrow flashloan
        let (mut base, loan) = vault.borrow_flashloan_base(id_from_address(@0x1), 1000, test.ctx());
        let return_base = base.split(999, test.ctx());
        vault.return_flashloan_base(id_from_address(@0x1), return_base, loan);

        abort(0)
    }

    #[test, expected_failure(abort_code = vault::EIncorrectQuantityReturned)]
    fun borrow_flashloan_incorrect_return_quote_e() {
        let mut test = begin(OWNER);

        let balance_manager_id = create_acct_and_share_with_funds(ALICE, 1000000 * constants::float_scaling(), &mut test);
        test.next_tx(ALICE);
        let mut vault = vault::empty<SPAM, USDC>();
        let settled_balances = balances::new(0, 0, 0);
        let owed_balances = balances::new(1000, 1000, 1000);
        let mut balance_manager = test.take_shared_by_id<BalanceManager>(balance_manager_id);
        let trade_proof = balance_manager.generate_proof_as_owner(test.ctx());

        // move funds into the vault
        vault.settle_balance_manager(settled_balances, owed_balances, &mut balance_manager, &trade_proof);

        // borrow flashloan
        let (mut quote, loan) = vault.borrow_flashloan_quote(id_from_address(@0x1), 1000, test.ctx());
        let return_quote = quote.split(999, test.ctx());
        vault.return_flashloan_quote(id_from_address(@0x1), return_quote, loan);

        abort(0)
    }
}

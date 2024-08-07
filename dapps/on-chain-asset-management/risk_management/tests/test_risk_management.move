// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module risk_management::test_risk_management {

    use sui::coin::{Self, Coin};
    use sui::sui::SUI;
    use sui::test_scenario::{Self, Scenario};
    use std::vector as vec;
    use risk_management::policy_config::{Self, SuperAdministratorCap, AdministratorCap ,SpenderCap, ApproverCap, RolesRegistry, Assets};
    use risk_management::transaction::{Self, TransactionRequest, TransactionApproval};

    const EWrongCoinValue: u64 = 0;
    const EWrongSpenderSpent : u64 = 1;

    fun create_approver(scenario: &mut Scenario, approver: address) {
        let administrator_cap = test_scenario::take_from_sender<AdministratorCap>(scenario);
        let roles_registry = test_scenario::take_shared<RolesRegistry>(scenario);
        let ctx = test_scenario::ctx(scenario);
        policy_config::create_approver(&administrator_cap, &mut roles_registry, approver, ctx);
        test_scenario::return_to_sender<AdministratorCap>(scenario, administrator_cap);
        test_scenario::return_shared(roles_registry);
    }

    fun approve_request(scenario: &mut Scenario) {
        let approver_cap = test_scenario::take_from_sender<ApproverCap>(scenario);
        let tx_request = test_scenario::take_shared<TransactionRequest>(scenario);
        let ctx = test_scenario::ctx(scenario);
        transaction::approve_request(&approver_cap, &mut tx_request, ctx);
        test_scenario::return_to_sender<ApproverCap>(scenario, approver_cap);
        test_scenario::return_shared(tx_request);
    }

    #[test]
    fun simple_transaction() {
        let super_admin = @0xBABE;
        let admin = @0xBAD;
        let admin_test = @0xBED;
        let approver1 = @0xCAFE;
        let approver2 = @0xDECAF;
        let spender = @0xFACE;
        let recipient = @0xBEEF;

        let scenario_val = test_scenario::begin(admin);
        let scenario = &mut scenario_val;

        // Call init function, transfer SuperAdministratorCap to the publisher
        test_scenario::next_tx(scenario, super_admin);
        {
            let ctx = test_scenario::ctx(scenario);
            policy_config::init_for_testing(ctx);
        };

        test_scenario::next_tx(scenario, super_admin);
        {
            let super_administrator_cap = test_scenario::take_from_sender<SuperAdministratorCap>(scenario);
            let admins = vec::empty();
            vec::push_back(&mut admins, admin);
            vec::push_back(&mut admins, admin_test);
            let ctx = test_scenario::ctx(scenario);
            policy_config::create_administrator(super_administrator_cap, admins, ctx);
        };

        // Admin creates first approver
        test_scenario::next_tx(scenario, admin);
        {
            create_approver(scenario, approver1);
        };

        // Admin creates second approver
        test_scenario::next_tx(scenario, admin);
        {
            create_approver(scenario, approver2);
        };

        // Admin creates spender without final approver
        test_scenario::next_tx(scenario, admin);
        {
            let administrator_cap = test_scenario::take_from_sender<AdministratorCap>(scenario);
            let roles_registry = test_scenario::take_shared<RolesRegistry>(scenario);
            let ctx = test_scenario::ctx(scenario);
            policy_config::create_spender(&administrator_cap, &mut roles_registry, spender, 200, 1, 300, ctx);
            test_scenario::return_to_sender<AdministratorCap>(scenario, administrator_cap);
            test_scenario::return_shared(roles_registry);
        };

        // Fund foundation balance
        test_scenario::next_tx(scenario, admin);
        {
            let assets = test_scenario::take_shared<Assets>(scenario);
            let ctx = test_scenario::ctx(scenario);
            let coin = coin::mint_for_testing<SUI>(10000, ctx);
            policy_config::top_up(&mut assets, coin, ctx);
            test_scenario::return_shared(assets);
        };

        // Spender creates a transaction request
        test_scenario::next_tx(scenario, spender);
        {
            let spender_cap = test_scenario::take_from_sender<SpenderCap>(scenario);
            let roles_registry = test_scenario::take_shared<RolesRegistry>(scenario);
            let ctx = test_scenario::ctx(scenario);
            transaction::initiate_transaction(&spender_cap, &roles_registry, 200, recipient, b"test", ctx);
            test_scenario::return_to_sender<SpenderCap>(scenario, spender_cap);
            test_scenario::return_shared(roles_registry);
        };

        // First approver approves transaction request
        test_scenario::next_tx(scenario, approver1);
        {
            approve_request(scenario);
        };

        // Second approver approves transaction request
        test_scenario::next_tx(scenario, approver2);
        {
            approve_request(scenario);
        };

        // Spender executes transaction
        test_scenario::next_tx(scenario, spender);
        {
            let spender_cap = test_scenario::take_from_sender<SpenderCap>(scenario);
            let tx_approval = test_scenario::take_from_sender<TransactionApproval>(scenario);
            let assets = test_scenario::take_shared<Assets>(scenario);
            let ctx = test_scenario::ctx(scenario);
            transaction::execute_transaction(&mut spender_cap, tx_approval, &mut assets, ctx);
            test_scenario::return_to_sender<SpenderCap>(scenario, spender_cap);
            test_scenario::return_shared(assets);
        };

        // Assertions
        test_scenario::next_tx(scenario, recipient);
        {
            let coin = test_scenario::take_from_sender<Coin<SUI>>(scenario);
            assert!(coin::value(&coin) == 200, 0);
            test_scenario::return_to_sender<Coin<SUI>>(scenario, coin);
        };

        test_scenario::next_tx(scenario, spender);
        {
            let spender_cap = test_scenario::take_from_sender<SpenderCap>(scenario);
            assert!(policy_config::get_spent(&spender_cap) == 200, 1);
            test_scenario::return_to_sender<SpenderCap>(scenario, spender_cap);
        };

        test_scenario::end(scenario_val);
    }
}


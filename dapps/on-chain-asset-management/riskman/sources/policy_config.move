// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module riskman::policy_config {

    use sui::tx_context::{Self, TxContext};
    use sui::object::{Self, UID};
    use sui::transfer;
    use std::vector as vec;

    struct AdministratorCap has key, store {
        id: UID,
    }

    struct SpenderCap has key, store {
        id: UID,
        policy: Policy,
        spent: u64,
    }

    struct ApproverCap has key, store {
        id: UID,
    }

    struct Policy has store {
        amount_limit: u64,
        time_limit: u64,
        //approvers_num: u64,
    }

    struct RolesRegistry has key, store {
        id: UID,
        administrators: vector<address>,
        spenders: vector<address>,
        approvers: vector<address>
    }

    // ======== Functions =========
    fun init(ctx: &mut TxContext) {
       transfer::transfer(AdministratorCap{id: object::new(ctx)}, 
                          tx_context::sender(ctx)
                        );
        transfer::share_object(RolesRegistry {
            id: object::new(ctx),
            administrators: vec::singleton(tx_context::sender(ctx)),
            spenders: vec::empty(),
            approvers: vec::empty()
        });
    }

    /// Optional
    // entry fun create_administrator(
    //     _: &AdministratorCap,
    //     reg: &mut RolesRegistry,
    //     recipient: address,
    //     ctx: &mut TxContext,
    // ) {

    // }

    entry fun create_spender(
        _: &AdministratorCap,
        reg: &mut RolesRegistry,
        recipient: address,
        amount_limit: u64,
        time_limit: u64,
        ctx: &mut TxContext,
    ) {
        assert!(tx_context::sender(ctx) != recipient, 0);
        transfer::transfer(SpenderCap{
                id: object::new(ctx),
                policy: Policy {amount_limit, time_limit},
                spent: 0,
            }, recipient
        );
        vec::push_back(&mut reg.spenders, recipient);
    }

    entry fun create_approver(
        _: &AdministratorCap,
        reg: &mut RolesRegistry,
        recipient: address,
        ctx: &mut TxContext,
    ) {
        transfer::transfer(ApproverCap{
                id: object::new(ctx)
            }, recipient
        );
        vec::push_back(&mut reg.approvers, recipient);
    }

    /// TODO
    // entry fun create_policy(
    //     _: &AdministratorCap,
    //     amount_limit: u64,
    //     time_limit: u64,
    // ) {}

    // entry fun delete_administrator(
    //     _: &AdministratorCap,
    //     reg: &mut RolesRegistry
    //     administrator: address,
    //     ctx: &mut TxContext,
    // ) {

    // }

    // entry fun delete_spender(
    //     _: &AdministratorCap,
    //     reg: &mut RolesRegistry
    //     spender: address,
    //     ctx: &mut TxContext,
    // ) {

    // }

    // entry fun delete_approver(
    //     _: &AdministratorCap,
    //     reg: &mut RolesRegistry
    //     approver: address,
    //     ctx: &mut TxContext,
    // ) {

    // }

    public fun get_amount_limit(
        spender_cap: &SpenderCap
    ) : u64 {
        spender_cap.policy.amount_limit
    }

    public fun get_amount_spent(
        spender_cap: &SpenderCap
    ) : u64 {
        spender_cap.spent
    }

    public fun get_time_limit(
        spender_cap: &SpenderCap
    ) : u64 {
        spender_cap.policy.time_limit
    }

    public fun add_spent(
        spender_cap: &mut SpenderCap,
        amount: u64
    ) {
        spender_cap.spent = spender_cap.spent + amount
    }
}
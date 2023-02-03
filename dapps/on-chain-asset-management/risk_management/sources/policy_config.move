// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module risk_management::policy_config {

    use sui::tx_context::{Self, TxContext};
    use sui::object::{Self, UID};
    use sui::coin::{Self, Coin};
    use sui::balance::{Self, Balance};
    use sui::sui::SUI;
    use sui::transfer;
    use std::vector as vec;
    use std::option::{Self, Option};

    const EAdministratorCannotBeSpender : u64 = 0;
    const ENotOriginalOwnerOfCapability : u64 = 1;
    const EFinalApproverDoesNotExist : u64 = 2;
    const ENoAdministratorsAssigned : u64 = 3;

    /// SuperAdministrator capability is created in init function and transfered to the publisher.
    /// It is used to assign the specified Administrators and then deleted forever.
    struct SuperAdministratorCap has key {
        id: UID
    }

    /// Administrator capability holder has the right to 
    /// create and assign spender and approver roles.
    struct AdministratorCap has key, store {
        id: UID,
        original_owner: address,
    }
    
    /// Spender capability holder has the right to initiate a transaction request.
    /// Spender's policy is specified by administrator once the capability is created
    struct SpenderCap has key, store {
        id: UID,
        original_owner: address,
        policy: Policy,
        spent: SpentPerEpoch,
    }

    /// Approver capability holder has the right to approve transaction requests.
    struct ApproverCap has key, store {
        id: UID,
        original_owner: address,
    }

    /// Assets struct contains the balance of the foundation, which is used for transactions.
    struct Assets has key {
        id: UID,
        foundation_balance: Balance<SUI>,
    }

    /// Policy is stored inside Spender capability to specify the thresholds of a spender.
    struct Policy has store {
        amount_limit: u64,
        time_limit: u64,
        velocity_limit: u64,
        final_approver: Option<address>,
    }

    /// Roles Registry is where the addresses that are assigned to a specific role.
    struct RolesRegistry has key, store {
        id: UID,
        administrators: vector<address>,
        spenders: vector<address>,
        approvers: vector<address>
    }

    struct SpentPerEpoch has store {
        amount: u64,
        epoch: u64,
    }

    // ======== Functions =========
    
    /// Once the smart contract is published, the publisher gets an Administrator capability.
    /// A Roles Registry object is shared containing the created administrator.
    /// Assets struct is also shared containing a balance with zero amount. 
    fun init(ctx: &mut TxContext) {
        transfer::transfer(SuperAdministratorCap{id: object::new(ctx)}, 
                          tx_context::sender(ctx)
                        );
        transfer::share_object(Assets {
            id: object::new(ctx),
            foundation_balance: balance::zero()
        });
    }

    #[test_only]
    public fun init_for_testing(ctx: &mut TxContext) {
        init(ctx)
    }

    public entry fun create_administrator(
        super_admin: SuperAdministratorCap,
        administrators: vector<address>,
        ctx: &mut TxContext,
    ) {
        assert!(vec::is_empty(&administrators) != true, 3);
        let registry = RolesRegistry {
            id: object::new(ctx),
            administrators: vec::empty(),
            spenders: vec::empty(),
            approvers: vec::empty()
            };
        while (!vec::is_empty(&administrators)) {  
            let admin = vec::pop_back(&mut administrators);  
            transfer::transfer(AdministratorCap{id: object::new(ctx), original_owner: admin}, 
                               admin
                            );
            vec::push_back(&mut registry.administrators, admin);
        };
        transfer::share_object(registry);
        let SuperAdministratorCap { id } = super_admin;
        object::delete(id);
    }

    /// Create spender function can only be called by an administrator, specifying the spender's thresholds.
    /// When a spender is created the Roles Registry gets updated.
    public entry fun create_spender(
        admin_cap: &AdministratorCap,
        reg: &mut RolesRegistry,
        recipient: address,
        amount_limit: u64,
        time_limit: u64,
        velocity_limit: u64,
        ctx: &mut TxContext,
    ) {
        assert!(admin_cap.original_owner == tx_context::sender(ctx), 1);
        assert!(tx_context::sender(ctx) != recipient, 0);
        transfer::transfer(SpenderCap{
                id: object::new(ctx),
                original_owner: recipient,
                policy: Policy {amount_limit, time_limit, velocity_limit, final_approver: option::none()},
                spent: SpentPerEpoch { amount: 0, epoch: tx_context::epoch(ctx)},
            }, recipient
        );
        vec::push_back(&mut reg.spenders, recipient);
    }

    /// Same function as above, with the difference 
    /// that a final_approver is specified for this kind of spender.
    public entry fun create_spender_with_final_approver(
        admin_cap: &AdministratorCap,
        reg: &mut RolesRegistry,
        recipient: address,
        amount_limit: u64,
        time_limit: u64,
        velocity_limit: u64,
        final_approver: address,
        ctx: &mut TxContext,
    ) {
        assert!(admin_cap.original_owner == tx_context::sender(ctx), 1);
        assert!(tx_context::sender(ctx) != recipient, 0);
        assert!(vec::contains(&reg.approvers, &final_approver) == true, 2);
        transfer::transfer(SpenderCap{
                id: object::new(ctx),
                original_owner: recipient,
                policy: Policy {amount_limit, time_limit, velocity_limit, final_approver: option::some(final_approver)},
                spent: SpentPerEpoch { amount: 0, epoch: tx_context::epoch(ctx)},
            }, recipient
        );
        vec::push_back(&mut reg.spenders, recipient);
    }

    /// Create approver function can only be called by an administrator.
    /// When a approver is created the Roles Registry gets updated.
    public entry fun create_approver(
        admin_cap: &AdministratorCap,
        reg: &mut RolesRegistry,
        recipient: address,
        ctx: &mut TxContext,
    ) {
        assert!(admin_cap.original_owner == tx_context::sender(ctx), 1);
        transfer::transfer(ApproverCap{
                id: object::new(ctx),
                original_owner: recipient,
            }, recipient
        );
        vec::push_back(&mut reg.approvers, recipient);
    }

    /// Function that can be called by whoever to top up foundation's balance.
    public entry fun top_up(
        assets: &mut Assets,
        coin: Coin<SUI>,
        _ctx: &mut TxContext,
    ) {
        coin::put(&mut assets.foundation_balance, coin)
    }

    public entry fun reset_spent(
        spender_cap: &mut SpenderCap,
        ctx: &mut TxContext,
    ) {
        if (tx_context::epoch(ctx) > spender_cap.spent.epoch) {
            spender_cap.spent.amount = 0;
            spender_cap.spent.epoch = tx_context::epoch(ctx);
        }
    }

    /// TODO
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

    public fun get_foundation_balance_mut(
        assets: &mut Assets
    ) : &mut Balance<SUI> {
        &mut assets.foundation_balance
    }

    public fun get_approvers_list(
        registry: &RolesRegistry
    ) : &vector<address> {
        &registry.approvers
    }

    public fun get_amount_limit(
        spender_cap: &SpenderCap
    ) : u64 {
        spender_cap.policy.amount_limit
    }

    public fun get_time_limit(
        spender_cap: &SpenderCap
    ) : u64 {
        spender_cap.policy.time_limit
    }

    public fun get_velocity_limit(
        spender_cap: &SpenderCap
    ) : u64 {
        spender_cap.policy.velocity_limit
    }

    public fun get_final_approver(
        spender_cap: &SpenderCap
    ) : Option<address> {
        spender_cap.policy.final_approver
    }

    public fun get_spender_original_owner(
        spender_cap: &SpenderCap
    ) : address {
        spender_cap.original_owner
    }

     public fun get_approver_original_owner(
        approver_cap: &ApproverCap
    ) : address {
        approver_cap.original_owner
    }

    public fun get_spent(
        spender_cap: &SpenderCap
    ) : u64 {
        spender_cap.spent.amount
    }

    public fun update_spent(
        spender_cap: &mut SpenderCap,
        amount: u64,
    ) {
        spender_cap.spent.amount = spender_cap.spent.amount + amount
    }
}
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module risk_management::transaction {

    use risk_management::policy_config::{Self, SpenderCap, ApproverCap, RolesRegistry, Assets};
    use sui::tx_context::{Self, TxContext};
    use sui::object::{Self, UID, ID};
    use sui::coin::{Self};
    use std::string::{Self, String};
    use sui::transfer;
    use std::vector as vec;
    use std::option::{Self, Option};

    const EAmountLimitExceeded : u64 = 0;
    const ETimeLimitExceeded : u64 = 1;
    const EAlreadyApprovedByThisApprover : u64 = 2;
    const ENotOriginalOwnerOfCapability : u64 = 3;
    const ESpenderCannotSendMoneyToHimself : u64 = 4;
    const EFinalApproverMustSignLast : u64 = 5;
    const ENotSpendersFinalApprover : u64 = 6;
    const EVelocityLimitExceeded : u64 = 7;

    /// A Transaction Request is created by a spender and hold all the required information
    /// for an approver to determine if its eligible to be approved or rejected 
    struct TransactionRequest has key, store {
        id: UID,
        amount: u64,
        time_limit: u64,
        spender: address,
        recipient: address,
        description: String,
        approvers_num: u64,
        final_approver: Option<address>,
        approved_by: vector<address>,
    }

    /// Transaction Approval is the key that an approver sends to the spender in order
    /// to execute the transaction requested
    struct TransactionApproval has key, store {
        id: UID,
        transaction_id: ID,
        amount: u64,
        spender: address,
        recipient: address,
        approvers: vector<address>,
    }

    /// Initiate transaction can only be called by a spender and a Transaction Request is shared,
    /// containing the amount to be transfered, the recipient and a description.
    /// M out of N rule is applied here by getting the number of existed approvers, dividing with 2 and adding 1.
    /// eg. if we have 4 approvers, 3 must approve this request.
    public entry fun initiate_transaction(
        spender_cap: &SpenderCap,
        registry: &RolesRegistry,
        amount: u64,
        recipient: address,
        description: vector<u8>,
        ctx: &mut TxContext,
    ) { 
        assert!(policy_config::get_spender_original_owner(spender_cap) == tx_context::sender(ctx), 3);
        assert!(recipient != tx_context::sender(ctx), 4);
        assert!(amount <= policy_config::get_amount_limit(spender_cap), 0);
        assert!(policy_config::get_spent(spender_cap) + amount <= policy_config::get_velocity_limit(spender_cap), 7);
        transfer::share_object(TransactionRequest {
            id: object::new(ctx),
            amount,
            time_limit: policy_config::get_time_limit(spender_cap) + tx_context::epoch(ctx),
            spender: tx_context::sender(ctx),
            recipient,
            description: string::utf8(description),
            approvers_num: (vec::length(policy_config::get_approvers_list(registry))/2) + 1,
            final_approver: policy_config::get_final_approver(spender_cap),
            approved_by: vec::empty()
        })
    }

    /// A Transaction Request can only be approved by approvers.
    /// If the spender is associated with a final approver, then only the final approver can
    /// create the Transaction Approval and send it to spender. Otherwise, when the m out of n
    /// rule is met, the Transaction Approval is send to spender.
    public entry fun approve_request(
        approver_cap: &ApproverCap,
        tx_request: &mut TransactionRequest,
        ctx: &mut TxContext,
    ) {
        assert!(policy_config::get_approver_original_owner(approver_cap) == tx_context::sender(ctx), 3);
        assert!(tx_context::epoch(ctx) <= tx_request.time_limit, 1);
        assert!(vec::contains(&tx_request.approved_by, &tx_context::sender(ctx)) == false, 2);
        if (vec::length(&tx_request.approved_by) < tx_request.approvers_num - 1) {
            assert!(option::get_with_default(&tx_request.final_approver, @0x0) != tx_context::sender(ctx), 5);
            vec::push_back(&mut tx_request.approved_by, tx_context::sender(ctx));
        } else {
            // if there is a final approver, assert that is the last to approve the request
            if (option::is_some(&tx_request.final_approver)) {
                assert!(&tx_context::sender(ctx) == option::borrow(&tx_request.final_approver), 6);
            };
            vec::push_back(&mut tx_request.approved_by, tx_context::sender(ctx));
            let newvector : vector<address> = tx_request.approved_by;
            transfer::transfer(TransactionApproval{
                id: object::new(ctx),
                transaction_id: object::uid_to_inner(&tx_request.id),
                amount: tx_request.amount,
                spender: tx_request.spender,
                recipient: tx_request.recipient,
                approvers: newvector,
            }, tx_request.spender);
        }
    }

    public entry fun reject_request(
        _: &ApproverCap,
        _tx_request: &TransactionRequest,
        _ctx: &mut TxContext,
    ) {
        // An event should be emitted informing that the request with
        // transaction_id: object::uid_to_inner(&tx_request.id) got rejected
    }

    /// Once the spender gets the Transaction Approval, the transaction can be executed.
    /// The funds are extracted from Assets and transfered to the recipient.
    public entry fun execute_transaction(
        spender_cap: &mut SpenderCap,
        tx_approval: TransactionApproval,
        assets: &mut Assets,
        ctx: &mut TxContext,
    ) {
        assert!(policy_config::get_spender_original_owner(spender_cap) == tx_context::sender(ctx), 3);
        assert!(tx_approval.amount <= policy_config::get_amount_limit(spender_cap), 0);
        assert!(policy_config::get_spent(spender_cap) <= policy_config::get_velocity_limit(spender_cap), 7);
        transfer::transfer(
            coin::take(policy_config::get_foundation_balance_mut(assets), tx_approval.amount, ctx) , 
            tx_approval.recipient
        );
        
        policy_config::update_spent(spender_cap, tx_approval.amount);

        //Unpack and delete the TransactionApproval
        let TransactionApproval { 
            id,
            transaction_id: _,
            amount: _,
            spender: _,
            recipient: _,
            approvers: _ } = tx_approval;
        object::delete(id)
    }

}
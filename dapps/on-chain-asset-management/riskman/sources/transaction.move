module riskman::transaction {

    use riskman::policy_config::{Self, SpenderCap, ApproverCap, Assets};
    use sui::tx_context::{Self, TxContext};
    use sui::object::{Self, UID, ID};
    use sui::coin::{Self};
    use std::string::{Self, String};
    use sui::transfer;

    const EAmountLimitExceeded : u64 = 0;
    const ETimeLimitExceeded : u64 = 1;
    const EAmountLimitExceededAfterApproval : u64 = 2;

    struct TransactionRequest has key, store {
        id: UID,
        amount: u64,
        time_limit: u64,
        spender: address,
        recipient: address,
        description: String,
    }

    struct TransactionApproval has key, store {
        id: UID,
        transaction_id: ID,
        amount: u64,
        spender: address,
        recipient: address,
        approver: address,
    }

    entry fun initiate_transaction(
        spender_cap: &SpenderCap,
        amount: u64,
        recipient: address,
        description: vector<u8>,
        ctx: &mut TxContext,
    ) { 
        assert!(amount <= policy_config::get_amount_limit(spender_cap) - policy_config::get_amount_spent(spender_cap), 0);
        transfer::share_object(TransactionRequest {
            id: object::new(ctx),
            amount,
            time_limit: policy_config::get_time_limit(spender_cap) + tx_context::epoch(ctx),
            spender: tx_context::sender(ctx),
            recipient,
            description: string::utf8(description)
        })
    }

    entry fun approve_request(
        _: &ApproverCap,
        tx_request: &TransactionRequest,
        ctx: &mut TxContext,
    ) {
        assert!(tx_context::epoch(ctx) <= tx_request.time_limit, 1);
        transfer::transfer(TransactionApproval{
            id: object::new(ctx),
            transaction_id: object::uid_to_inner(&tx_request.id),
            amount: tx_request.amount,
            spender: tx_request.spender,
            recipient: tx_request.recipient,
            approver: tx_context::sender(ctx)
        }, tx_request.spender);
    }

    entry fun reject_request(
        _: &ApproverCap,
        _tx_request: &TransactionRequest,
        _ctx: &mut TxContext,
    ) {
        // An event should be emitted informing that the request with
        // transaction_id: object::uid_to_inner(&tx_request.id) got rejected
    }

    entry fun execute_transaction(
        spender_cap: &mut SpenderCap,
        tx_approval: TransactionApproval,
        assets: &mut Assets,
        ctx: &mut TxContext,
    ) {
        assert!(tx_approval.amount <= policy_config::get_amount_limit(spender_cap) - policy_config::get_amount_spent(spender_cap), 2);
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
            approver: _ } = tx_approval;
        object::delete(id)
    }

}
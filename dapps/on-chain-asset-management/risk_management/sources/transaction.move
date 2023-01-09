module risk_management::transaction {

    use risk_management::policy_config::{Self, SpenderCap, ApproverCap, Assets};
    use sui::tx_context::{Self, TxContext};
    use sui::object::{Self, UID, ID};
    use sui::coin::{Self};
    use std::string::{Self, String};
    use sui::transfer;
    use std::vector as vec;

    const EAmountLimitExceeded : u64 = 0;
    const ETimeLimitExceeded : u64 = 1;
    const EAlreadyApprovedByThisApprover : u64 = 2;
    const ENotOriginalOwnerOfCapability : u64 = 3;
    const ESpenderCannotSendMoneyToHimself : u64 = 4;

    struct TransactionRequest has key, store {
        id: UID,
        amount: u64,
        time_limit: u64,
        spender: address,
        recipient: address,
        description: String,
        approvers_num: u64,
        approved_by: vector<address>,
    }

    struct TransactionApproval has key, store {
        id: UID,
        transaction_id: ID,
        amount: u64,
        spender: address,
        recipient: address,
        approvers: vector<address>,
    }

    entry fun initiate_transaction(
        spender_cap: &SpenderCap,
        amount: u64,
        recipient: address,
        description: vector<u8>,
        ctx: &mut TxContext,
    ) { 
        assert!(policy_config::get_spender_original_owner(spender_cap) == tx_context::sender(ctx), 3);
        assert!(recipient != tx_context::sender(ctx), 4);
        assert!(amount <= policy_config::get_amount_limit(spender_cap), 0);
        transfer::share_object(TransactionRequest {
            id: object::new(ctx),
            amount,
            time_limit: policy_config::get_time_limit(spender_cap) + tx_context::epoch(ctx),
            spender: tx_context::sender(ctx),
            recipient,
            description: string::utf8(description),
            approvers_num: policy_config::get_approvers_num(spender_cap),
            approved_by: vec::empty()
        })
    }

    entry fun approve_request(
        approver_cap: &ApproverCap,
        tx_request: &mut TransactionRequest,
        ctx: &mut TxContext,
    ) {
        assert!(policy_config::get_approver_original_owner(approver_cap) == tx_context::sender(ctx), 3);
        assert!(tx_context::epoch(ctx) <= tx_request.time_limit, 1);
        assert!(vec::contains(&tx_request.approved_by, &tx_context::sender(ctx)) == false, 2);
        if (vec::length(&tx_request.approved_by) < tx_request.approvers_num - 1) {
            vec::push_back(&mut tx_request.approved_by, tx_context::sender(ctx))
        } else {
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
        assert!(policy_config::get_spender_original_owner(spender_cap) == tx_context::sender(ctx), 3);
        assert!(tx_approval.amount <= policy_config::get_amount_limit(spender_cap), 0);
        transfer::transfer(
            coin::take(policy_config::get_foundation_balance_mut(assets), tx_approval.amount, ctx) , 
            tx_approval.recipient
        );
        
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
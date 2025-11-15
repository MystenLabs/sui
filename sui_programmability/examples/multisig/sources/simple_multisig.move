// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Simple multisig wallet module for shared fund management.
///
/// Multiple owners can propose and approve transactions.
/// A threshold of approvals is required to execute.
module multisig::simple_multisig {
    use sui::object::{Self, Info, UID};
    use sui::coin::{Self, Coin};
    use sui::balance::{Self, Balance};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};
    use sui::vec_map::{Self, VecMap};
    use std::vector;

    /// Error codes
    const ENotOwner: u64 = 0;
    const EAlreadyApproved: u64 = 1;
    const ENotEnoughApprovals: u64 = 2;
    const EAlreadyExecuted: u64 = 3;
    const EInvalidThreshold: u64 = 4;

    /// A multisig wallet
    struct Wallet<phantom T> has key {
        id: UID,
        owners: vector<address>,
        threshold: u64,
        balance: Balance<T>,
        proposal_count: u64,
    }

    /// A pending transaction proposal
    struct Proposal<phantom T> has key {
        id: UID,
        wallet_id: address,
        recipient: address,
        amount: u64,
        approvals: VecMap<address, bool>,
        executed: bool,
        proposer: address,
    }

    /// Create a new multisig wallet
    public entry fun create_wallet<T>(
        owners: vector<address>,
        threshold: u64,
        ctx: &mut TxContext
    ) {
        let owner_count = vector::length(&owners);
        assert!(threshold > 0 && threshold <= owner_count, EInvalidThreshold);

        let wallet = Wallet<T> {
            id: object::new(ctx),
            owners,
            threshold,
            balance: balance::zero(),
            proposal_count: 0,
        };

        transfer::share_object(wallet);
    }

    /// Deposit funds to the wallet
    public entry fun deposit<T>(
        wallet: &mut Wallet<T>,
        payment: Coin<T>,
        _ctx: &mut TxContext
    ) {
        let deposit_balance = coin::into_balance(payment);
        balance::join(&mut wallet.balance, deposit_balance);
    }

    /// Propose a transaction
    public entry fun propose<T>(
        wallet: &mut Wallet<T>,
        recipient: address,
        amount: u64,
        ctx: &mut TxContext
    ) {
        let sender = tx_context::sender(ctx);
        assert!(is_owner(wallet, sender), ENotOwner);

        wallet.proposal_count = wallet.proposal_count + 1;

        let proposal = Proposal<T> {
            id: object::new(ctx),
            wallet_id: object::id_address(wallet),
            recipient,
            amount,
            approvals: vec_map::empty(),
            executed: false,
            proposer: sender,
        };

        // Proposer automatically approves
        vec_map::insert(&mut proposal.approvals, sender, true);

        transfer::share_object(proposal);
    }

    /// Approve a proposal
    public entry fun approve<T>(
        wallet: &Wallet<T>,
        proposal: &mut Proposal<T>,
        ctx: &mut TxContext
    ) {
        let sender = tx_context::sender(ctx);
        assert!(is_owner(wallet, sender), ENotOwner);
        assert!(!proposal.executed, EAlreadyExecuted);

        // Check if already approved
        if (vec_map::contains(&proposal.approvals, &sender)) {
            assert!(false, EAlreadyApproved);
        };

        vec_map::insert(&mut proposal.approvals, sender, true);
    }

    /// Execute a proposal if threshold is met
    public entry fun execute<T>(
        wallet: &mut Wallet<T>,
        proposal: &mut Proposal<T>,
        ctx: &mut TxContext
    ) {
        assert!(!proposal.executed, EAlreadyExecuted);

        // Check if threshold is met
        let approval_count = vec_map::size(&proposal.approvals);
        assert!(approval_count >= wallet.threshold, ENotEnoughApprovals);

        // Check sufficient balance
        assert!(balance::value(&wallet.balance) >= proposal.amount, 0);

        // Execute transfer
        let payment = coin::take(&mut wallet.balance, proposal.amount, ctx);
        transfer::transfer(payment, proposal.recipient);

        proposal.executed = true;
    }

    /// Helper function to check if address is an owner
    fun is_owner<T>(wallet: &Wallet<T>, addr: address): bool {
        vector::contains(&wallet.owners, &addr)
    }

    /// View functions

    /// Get wallet info
    public fun get_wallet_info<T>(wallet: &Wallet<T>): (u64, u64, u64) {
        (
            vector::length(&wallet.owners),
            wallet.threshold,
            balance::value(&wallet.balance)
        )
    }

    /// Get proposal info
    public fun get_proposal_info<T>(proposal: &Proposal<T>): (address, u64, u64, bool) {
        (
            proposal.recipient,
            proposal.amount,
            vec_map::size(&proposal.approvals),
            proposal.executed
        )
    }

    /// Check if address has approved
    public fun has_approved<T>(proposal: &Proposal<T>, addr: address): bool {
        vec_map::contains(&proposal.approvals, &addr)
    }

    /// Get approval count
    public fun approval_count<T>(proposal: &Proposal<T>): u64 {
        vec_map::size(&proposal.approvals)
    }

    /// Check if proposal is ready to execute
    public fun is_ready<T>(wallet: &Wallet<T>, proposal: &Proposal<T>): bool {
        !proposal.executed &&
        vec_map::size(&proposal.approvals) >= wallet.threshold &&
        balance::value(&wallet.balance) >= proposal.amount
    }
}

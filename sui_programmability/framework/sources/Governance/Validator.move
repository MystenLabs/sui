// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module Sui::Validator {
    use Std::ASCII::{Self, String};
    use Std::Option::{Self, Option};

    use Sui::Coin::{Self, Coin};
    use Sui::ID::{Self, VersionedID};
    use Sui::SUI::SUI;
    use Sui::Transfer;
    use Sui::TxContext::{Self, TxContext};

    friend Sui::Genesis;
    friend Sui::ValidatorSet;

    /// This happens when someone tries to destroy a Validator object with sui_address
    /// that doesn't match the sender address.
    const EADDRESS_MISMATCH: u64 = 0;

    /// This can happen when a validator tries to add or withdraw stake but the resulting
    /// stake no longer meets validator stake requirement.
    const EINVALID_STAKE_AMOUNT: u64 = 1;

    /// This indicates inconsistent internal state, which shouldn't happen and if so is a bug.
    const EINCONSISTENT_STATE: u64 = 2;


    struct Validator has key, store {
        id: VersionedID,
        /// The Sui Address of the validator. This is the sender that created the Validator object,
        /// and also the address to send validator/coins to during withdraws.
        sui_address: address,
        /// A unique human-readable name of this validator.
        name: String,
        /// The IP address of the validator (could also contain extra info such as port, DNS and etc.).
        ip_address: vector<u8>,
        /// The current active stake. This will not change during an epoch. It can only
        /// be updated at the end of epoch.
        stake: Coin<SUI>,
        /// Pending stake deposits. It will be put into `stake` at the end of epoch.
        pending_stake: Option<Coin<SUI>>,
        /// Pending withdraw amount, processed at end of epoch.
        pending_withdraw: u64,
    }

    public(script) fun create(
        init_stake: Coin<SUI>,
        name: vector<u8>,
        ip_address: vector<u8>,
        ctx: &mut TxContext,
    ) {
        let sender = TxContext::sender(ctx);
        let validator = new(
            sender,
            name,
            ip_address,
            init_stake,
            ctx,
        );
        Transfer::transfer(validator, sender);
    }

    public(script) fun destroy(
        self: Validator,
        ctx: &mut TxContext,
    ) {
        let sender = TxContext::sender(ctx);
        assert!(self.sui_address == sender, EADDRESS_MISMATCH);

        // This must hold since only a non-active validator can be
        // directly owned and passed by value to `destroy`.
        check_non_active_validator_invariants(&self);

        let Validator { id, sui_address: _, name: _, ip_address: _, stake, pending_stake, pending_withdraw: _ } = self;
        Transfer::transfer(stake, sender);
        ID::delete(id);
        Option::destroy_none(pending_stake);
    }

    public(script) fun add_stake(
        self: &mut Validator,
        new_stake: Coin<SUI>,
        _ctx: &mut TxContext,
    ) {
        Coin::join(&mut self.stake, new_stake)
    }

    public(script) fun withdraw_stake(
        self: &mut Validator,
        withdraw_amount: u64,
        ctx: &mut TxContext,
    ) {
        let coin = Coin::withdraw(&mut self.stake, withdraw_amount, ctx);
        Coin::transfer(coin, TxContext::sender(ctx))
    }


    public(friend) fun new(
        sui_address: address,
        name: vector<u8>,
        ip_address: vector<u8>,
        stake: Coin<SUI>,
        ctx: &mut TxContext,
    ): Validator {
        Validator {
            id: TxContext::new_id(ctx),
            sui_address,
            name: ASCII::string(name),
            ip_address,
            stake,
            pending_stake: Option::none(),
            pending_withdraw: 0,
        }
    }

    /// Called by `ValidatorSet`, to send back a Validator object to its address.
    /// This happens when a validator is withdrawn or does not quality.
    public(friend) fun send_back(self: Validator) {
        check_non_active_validator_invariants(&self);
        let owner = self.sui_address;
        Transfer::transfer(self, owner)
    }

    /// Add stake to an active validator. The new stake is added to the pending_stake field,
    /// which will be processed at the end of epoch.
    public(friend) fun request_add_stake(
        self: &mut Validator,
        new_stake: Coin<SUI>,
        max_validator_stake: u64,
    ) {
        let cur_stake = Coin::value(&self.stake);
        if (Option::is_none(&self.pending_stake)) {
            assert!(
                cur_stake + Coin::value(&new_stake) <= max_validator_stake,
                EINVALID_STAKE_AMOUNT
            );
            Option::fill(&mut self.pending_stake, new_stake)
        } else {
            let pending_stake = Option::extract(&mut self.pending_stake);
            Coin::join(&mut pending_stake, new_stake);
            assert!(
                cur_stake + Coin::value(&pending_stake) <= max_validator_stake,
                EINVALID_STAKE_AMOUNT
            );
            Option::fill(&mut self.pending_stake, pending_stake);
        }
    }

    /// Withdraw stake from an active validator. Since it's active, we need
    /// to add it to the pending withdraw amount and process it at the end
    /// of epoch. We also need to make sure there is sufficient amount to withdraw.
    public(friend) fun request_withdraw_stake(
        self: &mut Validator,
        withdraw_amount: u64,
        min_validator_stake: u64,
    ) {
        self.pending_withdraw = self.pending_withdraw + withdraw_amount;

        let pending_stake_amount = if (Option::is_none(&self.pending_stake)) {
            0
        } else {
            Coin::value(Option::borrow(&self.pending_stake))
        };
        let total_stake = Coin::value(&self.stake) + pending_stake_amount;
        assert!(total_stake >= self.pending_withdraw + min_validator_stake, EINVALID_STAKE_AMOUNT);
    }

    /// Process pending stake and pending withdraws.
    public(friend) fun adjust_stake(self: &mut Validator, ctx: &mut TxContext) {
        if (Option::is_some(&self.pending_stake)) {
            let pending_stake = Option::extract(&mut self.pending_stake);
            Coin::join(&mut self.stake, pending_stake);
        };
        if (self.pending_withdraw > 0) {
            let coin = Coin::withdraw(&mut self.stake, self.pending_withdraw, ctx);
            Coin::transfer(coin, TxContext::sender(ctx));
            self.pending_withdraw = 0;
        }
    }


    public fun get_sui_address(self: &Validator): address {
        self.sui_address
    }

    public fun get_stake_amount(self: &Validator): u64 {
        Coin::value(&self.stake)
    }

    public fun is_duplicate(self: &Validator, other: &Validator): bool {
         self.sui_address == other.sui_address
            || self.name == other.name
            || self.ip_address == other.ip_address
    }

    /// For a non-active validator, it should never have any pending deposit or withdraws.
    /// Any active validator will first process those pending stake changes before they
    /// can be removed from the active validator list.
    /// TODO: Check this in Move Prover.
    fun check_non_active_validator_invariants(self: &Validator) {
        assert!(
            Option::is_none(&self.pending_stake)
                && self.pending_withdraw == 0,
            EINCONSISTENT_STATE
        );
    }
}
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::validator {
    use std::ascii;
    use std::vector;
    use std::bcs;

    use sui::balance::{Self, Balance};
    use sui::sui::SUI;
    use sui::tx_context::{Self, TxContext};
    use sui::stake;
    use sui::stake::Stake;
    use sui::epoch_time_lock::EpochTimeLock;
    use std::option::Option;
    use sui::bls12381::bls12381_min_sig_verify_with_domain;
    use sui::staking_pool::{Self, Delegation, PoolTokenExchangeRate, StakedSui, StakingPool};

    friend sui::genesis;
    friend sui::sui_system;
    friend sui::validator_set;

    #[test_only]
    friend sui::validator_tests;
    #[test_only]
    friend sui::validator_set_tests;
    #[test_only]
    friend sui::governance_test_utils;

    struct ValidatorMetadata has store, drop, copy {
        /// The Sui Address of the validator. This is the sender that created the Validator object,
        /// and also the address to send validator/coins to during withdraws.
        sui_address: address,
        /// The public key bytes corresponding to the private key that the validator
        /// holds to sign transactions. For now, this is the same as AuthorityName.
        pubkey_bytes: vector<u8>,
        /// The public key bytes corresponding to the private key that the validator
        /// uses to establish TLS connections
        network_pubkey_bytes: vector<u8>,
        /// The public key bytes correstponding to the Narwhal Worker
        worker_pubkey_bytes: vector<u8>,
        /// This is a proof that the validator has ownership of the private key
        proof_of_possession: vector<u8>,
        /// A unique human-readable name of this validator.
        name: vector<u8>,
        description: vector<u8>,
        image_url: vector<u8>,
        project_url: vector<u8>,
        /// The network address of the validator (could also contain extra info such as port, DNS and etc.).
        net_address: vector<u8>,
        /// The address of the narwhal primary
        consensus_address: vector<u8>,
        /// The address of the narwhal worker
        worker_address: vector<u8>,
        /// Total amount of validator stake that would be active in the next epoch.
        next_epoch_stake: u64,
        /// Total amount of delegated stake that would be active in the next epoch.
        next_epoch_delegation: u64,
        /// This validator's gas price quote for the next epoch.
        next_epoch_gas_price: u64,
        /// The commission rate of the validator starting the next epoch, in basis point.
        next_epoch_commission_rate: u64,
    }

    struct Validator has store {
        /// Summary of the validator.
        metadata: ValidatorMetadata,
        /// The current active stake amount. This will not change during an epoch. It can only
        /// be updated at the end of epoch.
        stake_amount: u64,
        /// Pending stake deposit amount, processed at end of epoch.
        pending_stake: u64,
        /// Pending withdraw amount, processed at end of epoch.
        pending_withdraw: u64,
        /// Gas price quote, updated only at end of epoch.
        gas_price: u64,
        /// Staking pool for the stakes delegated to this validator.
        delegation_staking_pool: StakingPool,
        /// Commission rate of the validator, in basis point.
        commission_rate: u64,
    }

    const PROOF_OF_POSSESSION_DOMAIN: vector<u8> = vector[107, 111, 115, 107];

    fun verify_proof_of_possession(
        proof_of_possession: vector<u8>,
        sui_address: address,
        pubkey_bytes: vector<u8>
    ) {
        // The proof of possession is the signature over ValidatorPK || AccountAddress.
        // This proves that the account address is owned by the holder of ValidatorPK, and ensures
        // that PK exists.
        let signed_bytes = pubkey_bytes;
        let address_bytes = bcs::to_bytes(&sui_address);
        vector::append(&mut signed_bytes, address_bytes);
        assert!(
            bls12381_min_sig_verify_with_domain(&proof_of_possession, &pubkey_bytes, signed_bytes, PROOF_OF_POSSESSION_DOMAIN) == true,
            0
        );
    }

    public(friend) fun new(
        sui_address: address,
        pubkey_bytes: vector<u8>,
        network_pubkey_bytes: vector<u8>,
        worker_pubkey_bytes: vector<u8>,
        proof_of_possession: vector<u8>,
        name: vector<u8>,
        description: vector<u8>,
        image_url: vector<u8>,
        project_url: vector<u8>,
        net_address: vector<u8>,
        consensus_address: vector<u8>,
        worker_address: vector<u8>,
        stake: Balance<SUI>,
        coin_locked_until_epoch: Option<EpochTimeLock>,
        gas_price: u64,
        commission_rate: u64,
        ctx: &mut TxContext
    ): Validator {
        assert!(
            // TODO: These constants are arbitrary, will adjust once we know more.
            vector::length(&net_address) <= 128 && vector::length(&name) <= 128 && vector::length(&pubkey_bytes) <= 128,
            0
        );
        verify_proof_of_possession(
            proof_of_possession,
            sui_address,
            pubkey_bytes
        );
        // Check that the name is human-readable.
        ascii::string(copy name);
        let stake_amount = balance::value(&stake);
        stake::create(stake, sui_address, coin_locked_until_epoch, ctx);
        Validator {
            metadata: ValidatorMetadata {
                sui_address,
                pubkey_bytes,
                network_pubkey_bytes,
                worker_pubkey_bytes,
                proof_of_possession,
                name,
                description,
                image_url,
                project_url,
                net_address,
                consensus_address,
                worker_address,
                next_epoch_stake: stake_amount,
                next_epoch_delegation: 0,
                next_epoch_gas_price: gas_price,
                next_epoch_commission_rate: commission_rate,
            },
            stake_amount,
            pending_stake: 0,
            pending_withdraw: 0,
            gas_price,
            delegation_staking_pool: staking_pool::new(sui_address, tx_context::epoch(ctx) + 1, ctx),
            commission_rate,
        }
    }

    public(friend) fun destroy(self: Validator, ctx: &mut TxContext) {
        let Validator {
            metadata: _,
            stake_amount: _,
            pending_stake: _,
            pending_withdraw: _,
            gas_price: _,
            delegation_staking_pool,
            commission_rate: _,
        } = self;
        staking_pool::deactivate_staking_pool(delegation_staking_pool, ctx);
    }

    /// Add stake to an active validator. The new stake is added to the pending_stake field,
    /// which will be processed at the end of epoch.
    public(friend) fun request_add_stake(
        self: &mut Validator,
        new_stake: Balance<SUI>,
        coin_locked_until_epoch: Option<EpochTimeLock>,
        ctx: &mut TxContext,
    ) {
        let new_stake_value = balance::value(&new_stake);
        self.pending_stake = self.pending_stake + new_stake_value;
        self.metadata.next_epoch_stake = self.metadata.next_epoch_stake + new_stake_value;
        stake::create(new_stake, self.metadata.sui_address, coin_locked_until_epoch, ctx);
    }

    /// Withdraw stake from an active validator. Since it's active, we need
    /// to add it to the pending withdraw amount and process it at the end
    /// of epoch. We also need to make sure there is sufficient amount to withdraw such that the validator's
    /// stake still satisfy the minimum requirement.
    public(friend) fun request_withdraw_stake(
        self: &mut Validator,
        stake: &mut Stake,
        withdraw_amount: u64,
        min_validator_stake: u64,
        ctx: &mut TxContext,
    ) {
        assert!(self.metadata.next_epoch_stake >= withdraw_amount + min_validator_stake, 0);
        self.pending_withdraw = self.pending_withdraw + withdraw_amount;
        self.metadata.next_epoch_stake = self.metadata.next_epoch_stake - withdraw_amount;
        stake::withdraw_stake(stake, withdraw_amount, ctx);
    }

    /// Process pending stake and pending withdraws, and update the gas price.
    public(friend) fun adjust_stake_and_gas_price(self: &mut Validator) {
        self.stake_amount = self.stake_amount + self.pending_stake - self.pending_withdraw;
        self.pending_stake = 0;
        self.pending_withdraw = 0;
        self.gas_price = self.metadata.next_epoch_gas_price;
        self.commission_rate = self.metadata.next_epoch_commission_rate;
        assert!(self.stake_amount == self.metadata.next_epoch_stake, 0);
    }

    /// Request to add delegation to the validator's staking pool, processed at the end of the epoch.
    public(friend) fun request_add_delegation(
        self: &mut Validator,
        delegated_stake: Balance<SUI>,
        locking_period: Option<EpochTimeLock>,
        delegator: address,
        ctx: &mut TxContext,
    ) {
        let delegate_amount = balance::value(&delegated_stake);
        assert!(delegate_amount > 0, 0);
        staking_pool::request_add_delegation(&mut self.delegation_staking_pool, delegated_stake, locking_period, delegator, ctx);
        self.metadata.next_epoch_delegation = self.metadata.next_epoch_delegation + delegate_amount;
    }

    /// Request to withdraw delegation from the validator's staking pool, processed at the end of the epoch.
    public(friend) fun request_withdraw_delegation(
        self: &mut Validator,
        delegation: Delegation,
        staked_sui: StakedSui,
        ctx: &mut TxContext,
    ) {
        let principal_withdraw_amount = staking_pool::request_withdraw_delegation(
                &mut self.delegation_staking_pool, delegation, staked_sui, ctx);
        decrease_next_epoch_delegation(self, principal_withdraw_amount);
    }

    /// Decrement the delegation amount for next epoch. Also called by `validator_set` when handling delegation switches.
    public(friend) fun decrease_next_epoch_delegation(self: &mut Validator, amount: u64) {
        self.metadata.next_epoch_delegation = self.metadata.next_epoch_delegation - amount;
    }

    /// Request to set new gas price for the next epoch.
    public(friend) fun request_set_gas_price(self: &mut Validator, new_price: u64) {
        self.metadata.next_epoch_gas_price = new_price;
    }

    public(friend) fun request_set_commission_rate(self: &mut Validator, new_commission_rate: u64) {
        self.metadata.next_epoch_commission_rate = new_commission_rate;
    }

    /// Deposit delegations rewards into the validator's staking pool, called at the end of the epoch.
    public(friend) fun deposit_delegation_rewards(self: &mut Validator, reward: Balance<SUI>) {
        self.metadata.next_epoch_delegation = self.metadata.next_epoch_delegation + balance::value(&reward);
        staking_pool::deposit_rewards(&mut self.delegation_staking_pool, reward);
    }

    /// Process pending delegations and withdraws, called at the end of the epoch.
    public(friend) fun process_pending_delegations_and_withdraws(self: &mut Validator, ctx: &mut TxContext) {
        staking_pool::process_pending_delegations(&mut self.delegation_staking_pool, ctx);
        let reward_withdraw_amount = staking_pool::process_pending_delegation_withdraws(
            &mut self.delegation_staking_pool, ctx);
        self.metadata.next_epoch_delegation = self.metadata.next_epoch_delegation - reward_withdraw_amount;
        // assert!(delegate_amount(self) == self.metadata.next_epoch_delegation, 0);
    }

    /// Called by `validator_set` for handling delegation switches.
    public(friend) fun get_staking_pool_mut_ref(self: &mut Validator) : &mut StakingPool {
        &mut self.delegation_staking_pool
    }

    public fun metadata(self: &Validator): &ValidatorMetadata {
        &self.metadata
    }

    public fun sui_address(self: &Validator): address {
        self.metadata.sui_address
    }

    public fun stake_amount(self: &Validator): u64 {
        self.stake_amount
    }

    public fun delegate_amount(self: &Validator): u64 {
        staking_pool::sui_balance(&self.delegation_staking_pool)
    }

    /// Return the total amount staked with this validator, including both validator stake and deledgated stake
    public fun total_stake(self: &Validator): u64 {
        stake_amount(self) + delegate_amount(self)
    }

    public fun pending_stake_amount(self: &Validator): u64 {
        self.pending_stake
    }

    public fun pending_withdraw(self: &Validator): u64 {
        self.pending_withdraw
    }

    public fun gas_price(self: &Validator): u64 {
        self.gas_price
    }

    public fun commission_rate(self: &Validator): u64 {
        self.commission_rate
    }

    public fun pool_token_exchange_rate(self: &Validator): PoolTokenExchangeRate {
        staking_pool::pool_token_exchange_rate(&self.delegation_staking_pool)
    }

    public fun is_duplicate(self: &Validator, other: &Validator): bool {
         self.metadata.sui_address == other.metadata.sui_address
            || self.metadata.name == other.metadata.name
            || self.metadata.net_address == other.metadata.net_address
            || self.metadata.pubkey_bytes == other.metadata.pubkey_bytes
    }

    // CAUTION: THIS CODE IS ONLY FOR TESTING AND THIS MACRO MUST NEVER EVER BE REMOVED.
    // Creates a validator - bypassing the proof of possession in check in the process.
    #[test_only]
    public(friend) fun new_for_testing(
        sui_address: address,
        pubkey_bytes: vector<u8>,
        network_pubkey_bytes: vector<u8>,
        worker_pubkey_bytes: vector<u8>,
        proof_of_possession: vector<u8>,
        name: vector<u8>,
        description: vector<u8>,
        image_url: vector<u8>,
        project_url: vector<u8>,
        net_address: vector<u8>,
        consensus_address: vector<u8>,
        worker_address: vector<u8>,
        stake: Balance<SUI>,
        coin_locked_until_epoch: Option<EpochTimeLock>,
        gas_price: u64,
        commission_rate: u64,
        ctx: &mut TxContext
    ): Validator {
        assert!(
            // TODO: These constants are arbitrary, will adjust once we know more.
            vector::length(&net_address) <= 128 && vector::length(&name) <= 128 && vector::length(&pubkey_bytes) <= 128,
            0
        );
        // Check that the name is human-readable.
        ascii::string(copy name);
        let stake_amount = balance::value(&stake);
        stake::create(stake, sui_address, coin_locked_until_epoch, ctx);
        Validator {
            metadata: ValidatorMetadata {
                sui_address,
                pubkey_bytes,
                network_pubkey_bytes,
                worker_pubkey_bytes,
                proof_of_possession,
                name,
                description,
                image_url,
                project_url,
                net_address,
                consensus_address,
                worker_address,
                next_epoch_stake: stake_amount,
                next_epoch_delegation: 0,
                next_epoch_gas_price: gas_price,
                next_epoch_commission_rate: commission_rate,
            },
            stake_amount,
            pending_stake: 0,
            pending_withdraw: 0,
            gas_price,
            delegation_staking_pool: staking_pool::new(sui_address, tx_context::epoch(ctx) + 1, ctx),
            commission_rate,
        }
    }
}

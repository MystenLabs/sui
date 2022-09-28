// Copyright (c) 2022, Mysten Labs, Inc.
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
    use sui::crypto::Self;
    use sui::staking_pool::{Self, Delegation, StakedSui, StakingPool};

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
        /// This is a proof that the validator has ownership of the private key
        proof_of_possession: vector<u8>,
        /// A unique human-readable name of this validator.
        name: vector<u8>,
        /// The network address of the validator (could also contain extra info such as port, DNS and etc.).
        net_address: vector<u8>,
        /// Total amount of validator stake that would be active in the next epoch.
        next_epoch_stake: u64,
        /// Total amount of delegated stake that would be active in the next epoch.
        next_epoch_delegation: u64,
        /// This validator's gas price quote for the next epoch.
        next_epoch_gas_price: u64,
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
            crypto::bls12381_verify_with_domain(proof_of_possession, pubkey_bytes, signed_bytes, PROOF_OF_POSSESSION_DOMAIN) == true,
            0
        );
    }

    public(friend) fun new(
        sui_address: address,
        pubkey_bytes: vector<u8>,
        network_pubkey_bytes: vector<u8>,
        proof_of_possession: vector<u8>,
        name: vector<u8>,
        net_address: vector<u8>,
        stake: Balance<SUI>,
        coin_locked_until_epoch: Option<EpochTimeLock>,
        gas_price: u64,
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
                proof_of_possession,
                name,
                net_address,
                next_epoch_stake: stake_amount,
                next_epoch_delegation: 0,
                next_epoch_gas_price: gas_price,
            },
            stake_amount,
            pending_stake: 0,
            pending_withdraw: 0,
            gas_price,
            delegation_staking_pool: staking_pool::new(sui_address, tx_context::epoch(ctx) + 1),
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

    /// Process pending stake and pending withdraws.
    public(friend) fun adjust_stake_and_gas_price(self: &mut Validator) {
        self.stake_amount = self.stake_amount + self.pending_stake - self.pending_withdraw;
        self.pending_stake = 0;
        self.pending_withdraw = 0;
        self.gas_price = self.metadata.next_epoch_gas_price;
        assert!(self.stake_amount == self.metadata.next_epoch_stake, 0);
    }

    public(friend) fun request_add_delegation(
        self: &mut Validator, 
        delegated_stake: Balance<SUI>,
        locking_period: Option<EpochTimeLock>,
        ctx: &mut TxContext,
    ) {
        let delegate_amount = balance::value(&delegated_stake);
        assert!(delegate_amount > 0, 0);
        staking_pool::request_add_delegation(&mut self.delegation_staking_pool, delegated_stake, locking_period, ctx);

        increase_next_epoch_delegation(self, delegate_amount);
    }

    public(friend) fun increase_next_epoch_delegation(self: &mut Validator, amount: u64) {
        self.metadata.next_epoch_delegation = self.metadata.next_epoch_delegation + amount;
    }

    public(friend) fun request_withdraw_delegation(
        self: &mut Validator, 
        delegation: &mut Delegation, 
        staked_sui: &mut StakedSui,
        withdraw_pool_token_amount: u64,
        ctx: &mut TxContext,
    ) {
        let withdraw_sui_amount = staking_pool::withdraw_stake(
                &mut self.delegation_staking_pool, delegation, staked_sui, withdraw_pool_token_amount, ctx);
        decrease_next_epoch_delegation(self, withdraw_sui_amount);
    }

    public(friend) fun decrease_next_epoch_delegation(self: &mut Validator, amount: u64) {
        self.metadata.next_epoch_delegation = self.metadata.next_epoch_delegation - amount;
    }

    public(friend) fun request_set_gas_price(self: &mut Validator, new_price: u64) {
        self.metadata.next_epoch_gas_price = new_price;
    }

    public(friend) fun distribute_rewards_and_new_delegations(self: &mut Validator, reward: Balance<SUI>, ctx: &mut TxContext) {
        self.metadata.next_epoch_delegation = self.metadata.next_epoch_delegation + balance::value(&reward);
        staking_pool::advance_epoch(&mut self.delegation_staking_pool, reward, ctx);
        assert!(delegate_amount(self) == self.metadata.next_epoch_delegation, 0);
    }

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

    public fun pending_stake_amount(self: &Validator): u64 {
        self.pending_stake
    }

    public fun pending_withdraw(self: &Validator): u64 {
        self.pending_withdraw
    }

    public fun gas_price(self: &Validator): u64 {
        self.gas_price
    }

    public fun is_duplicate(self: &Validator, other: &Validator): bool {
         self.metadata.sui_address == other.metadata.sui_address
            || self.metadata.name == other.metadata.name
            || self.metadata.net_address == other.metadata.net_address
    }

    // CAUTION: THIS CODE IS ONLY FOR TESTING AND THIS MACRO MUST NEVER EVER BE REMOVED.
    // Creates a validator - bypassing the proof of possession in check in the process.
    #[test_only]
    public(friend) fun new_for_testing(
        sui_address: address,
        pubkey_bytes: vector<u8>,
        network_pubkey_bytes: vector<u8>,
        proof_of_possession: vector<u8>,
        name: vector<u8>,
        net_address: vector<u8>,
        stake: Balance<SUI>,
        coin_locked_until_epoch: Option<EpochTimeLock>,
        gas_price: u64,
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
                proof_of_possession,
                name,
                net_address,
                next_epoch_stake: stake_amount,
                next_epoch_delegation: 0,
                next_epoch_gas_price: gas_price,
            },
            stake_amount,
            pending_stake: 0,
            pending_withdraw: 0,
            gas_price,
            delegation_staking_pool: staking_pool::new(sui_address, tx_context::epoch(ctx) + 1),
        }
    }
}

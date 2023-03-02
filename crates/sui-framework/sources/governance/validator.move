// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::validator {
    use std::ascii;
    use std::vector;
    use std::bcs;

    use sui::balance::{Self, Balance};
    use sui::bcs::to_bytes;
    use sui::sui::SUI;
    use sui::tx_context::{Self, TxContext};
    use sui::epoch_time_lock::EpochTimeLock;
    use sui::object::{Self, ID};
    use std::option::Option;
    use sui::bls12381::bls12381_min_sig_verify_with_domain;
    use sui::staking_pool::{Self, PoolTokenExchangeRate, StakedSui, StakingPool};
    use std::string::{Self, String};
    use sui::url::Url;
    use sui::url;
    friend sui::genesis;
    friend sui::sui_system;
    friend sui::validator_set;
    friend sui::voting_power;

    #[test_only]
    friend sui::validator_tests;
    #[test_only]
    friend sui::validator_set_tests;
    #[test_only]
    friend sui::governance_test_utils;

    /// Invalid pubkey_bytes field in ValidatorMetadata
    const EMetadataInvalidPubKey: u64 = 1;

    /// Invalid network_pubkey_bytes field in ValidatorMetadata
    const EMetadataInvalidNetPubkey: u64 = 2;

    /// Invalid worker_pubkey_bytes field in ValidatorMetadata
    const EMetadataInvalidWorkerPubKey: u64 = 3;

    /// Invalid net_address field in ValidatorMetadata
    const EMetadataInvalidNetAddr: u64 = 4;

    /// Invalid p2p_address field in ValidatorMetadata
    const EMetadataInvalidP2pAddr: u64 = 5;

    /// Invalid consensus_address field in ValidatorMetadata
    const EMetadataInvalidConsensusAddr: u64 = 6;

    /// Invalidworker_address field in ValidatorMetadata
    const EMetadataInvalidWorkerAddr: u64 = 7;


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
        name: String,
        description: String,
        image_url: Url,
        project_url: Url,
        /// The network address of the validator (could also contain extra info such as port, DNS and etc.).
        net_address: vector<u8>,
        /// The address of the validator used for p2p activities such as state sync (could also contain extra info such as port, DNS and etc.).
        p2p_address: vector<u8>,
        /// The address of the narwhal primary
        consensus_address: vector<u8>,
        /// The address of the narwhal worker
        worker_address: vector<u8>,
    }

    struct Validator has store {
        /// Summary of the validator.
        metadata: ValidatorMetadata,
        /// The voting power of this validator, which might be different from its
        /// stake amount.
        voting_power: u64,
        /// Gas price quote, updated only at end of epoch.
        gas_price: u64,
        /// Staking pool for the stakes delegated to this validator.
        staking_pool: StakingPool,
        /// Commission rate of the validator, in basis point.
        commission_rate: u64,
        /// Total amount of validator stake that would be active in the next epoch.
        next_epoch_stake: u64,
        /// Total amount of delegated stake that would be active in the next epoch.
        next_epoch_delegation: u64,
        /// This validator's gas price quote for the next epoch.
        next_epoch_gas_price: u64,
        /// The commission rate of the validator starting the next epoch, in basis point.
        next_epoch_commission_rate: u64,
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
        let address_bytes = to_bytes(&sui_address);
        vector::append(&mut signed_bytes, address_bytes);
        assert!(
            bls12381_min_sig_verify_with_domain(&proof_of_possession, &pubkey_bytes, signed_bytes, PROOF_OF_POSSESSION_DOMAIN) == true,
            0
        );
    }
    public(friend) fun new_metadata(
        sui_address: address,
        pubkey_bytes: vector<u8>,
        network_pubkey_bytes: vector<u8>,
        worker_pubkey_bytes: vector<u8>,
        proof_of_possession: vector<u8>,
        name: String,
        description: String,
        image_url: Url,
        project_url: Url,
        net_address: vector<u8>,
        p2p_address: vector<u8>,
        consensus_address: vector<u8>,
        worker_address: vector<u8>,
    ): ValidatorMetadata {
        let metadata = ValidatorMetadata {
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
            p2p_address,
            consensus_address,
            worker_address,
        };
        metadata
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
        p2p_address: vector<u8>,
        consensus_address: vector<u8>,
        worker_address: vector<u8>,
        stake: Balance<SUI>,
        coin_locked_until_epoch: Option<EpochTimeLock>,
        gas_price: u64,
        commission_rate: u64,
        starting_epoch: u64,
        ctx: &mut TxContext
    ): Validator {
        assert!(
            // TODO: These constants are arbitrary, will adjust once we know more.
            vector::length(&net_address) <= 128
                && vector::length(&p2p_address) <= 128
                && vector::length(&name) <= 128
                && vector::length(&description) <= 150
                && vector::length(&pubkey_bytes) <= 128,
            0
        );
        verify_proof_of_possession(
            proof_of_possession,
            sui_address,
            pubkey_bytes
        );
        let stake_amount = balance::value(&stake);
        let metadata =  new_metadata(
            sui_address,
            pubkey_bytes,
            network_pubkey_bytes,
            worker_pubkey_bytes,
            proof_of_possession,
            string::from_ascii(ascii::string(name)),
            string::from_ascii(ascii::string(description)),
            url::new_unsafe_from_bytes(image_url),
            url::new_unsafe_from_bytes(project_url),
            net_address,
            p2p_address,
            consensus_address,
            worker_address,
        );

        validate_metadata(&metadata);
        let staking_pool = staking_pool::new(starting_epoch, ctx);
        // Add the validator's starting stake to the staking pool.
        staking_pool::request_add_delegation(&mut staking_pool, stake, coin_locked_until_epoch, sui_address, sui_address, starting_epoch, ctx);
        // We immediately process this delegation as they are at validator setup time and this is the validator staking with itself.
        staking_pool::process_pending_delegation(&mut staking_pool, starting_epoch);
        Validator {
            metadata,
            // Initialize the voting power to be the same as the stake amount.
            // At the epoch change where this validator is actually added to the
            // active validator set, the voting power will be updated accordingly.
            voting_power: stake_amount,
            gas_price,
            staking_pool,
            commission_rate,
            next_epoch_stake: stake_amount,
            next_epoch_delegation: 0,
            next_epoch_gas_price: gas_price,
            next_epoch_commission_rate: commission_rate,
        }
    }

    public(friend) fun destroy(self: Validator, ctx: &mut TxContext) {
        let Validator {
            metadata: _,
            voting_power: _,
            gas_price: _,
            staking_pool,
            commission_rate: _,
            next_epoch_stake: _,
            next_epoch_delegation: _,
            next_epoch_gas_price: _,
            next_epoch_commission_rate: _,
        } = self;
        staking_pool::deactivate_staking_pool(staking_pool, ctx);
    }

    /// Process pending stake and pending withdraws, and update the gas price.
    public(friend) fun adjust_stake_and_gas_price(self: &mut Validator) {
        self.gas_price = self.next_epoch_gas_price;
        self.commission_rate = self.next_epoch_commission_rate;
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
        let delegation_epoch = tx_context::epoch(ctx) + 1;
        staking_pool::request_add_delegation(
            &mut self.staking_pool, delegated_stake, locking_period, self.metadata.sui_address, delegator, delegation_epoch, ctx
        );
        self.next_epoch_delegation = self.next_epoch_delegation + delegate_amount;
    }

    /// Request to withdraw delegation from the validator's staking pool, processed at the end of the epoch.
    public(friend) fun request_withdraw_delegation(
        self: &mut Validator,
        staked_sui: StakedSui,
        ctx: &mut TxContext,
    ) {
        let principal_withdraw_amount = staking_pool::request_withdraw_delegation(
                &mut self.staking_pool, staked_sui, ctx);
        decrease_next_epoch_delegation(self, principal_withdraw_amount);
    }

    /// Decrement the delegation amount for next epoch. Also called by `validator_set` when handling delegation switches.
    public(friend) fun decrease_next_epoch_delegation(self: &mut Validator, amount: u64) {
        self.next_epoch_delegation = self.next_epoch_delegation - amount;
    }

    /// Request to set new gas price for the next epoch.
    public(friend) fun request_set_gas_price(self: &mut Validator, new_price: u64) {
        self.next_epoch_gas_price = new_price;
    }

    public(friend) fun request_set_commission_rate(self: &mut Validator, new_commission_rate: u64) {
        self.next_epoch_commission_rate = new_commission_rate;
    }

    /// Deposit delegations rewards into the validator's staking pool, called at the end of the epoch.
    public(friend) fun deposit_delegation_rewards(self: &mut Validator, reward: Balance<SUI>, new_epoch: u64) {
        self.next_epoch_delegation = self.next_epoch_delegation + balance::value(&reward);
        staking_pool::deposit_rewards(&mut self.staking_pool, reward, new_epoch);
    }

    /// Process pending delegations and withdraws, called at the end of the epoch.
    public(friend) fun process_pending_delegations_and_withdraws(self: &mut Validator, ctx: &mut TxContext) {
        let new_epoch = tx_context::epoch(ctx) + 1;
        let reward_withdraw_amount = staking_pool::process_pending_delegation_withdraws(
            &mut self.staking_pool, ctx);
        self.next_epoch_delegation = self.next_epoch_delegation - reward_withdraw_amount;
        staking_pool::process_pending_delegation(&mut self.staking_pool, new_epoch);
        // TODO: consider bringing this assert back when we are more confident.
        // assert!(delegate_amount(self) == self.metadata.next_epoch_delegation, 0);
    }

    /// Called by `validator_set` for handling delegation switches.
    public(friend) fun get_staking_pool_mut_ref(self: &mut Validator) : &mut StakingPool {
        &mut self.staking_pool
    }

    public fun metadata(self: &Validator): &ValidatorMetadata {
        &self.metadata
    }

    public fun sui_address(self: &Validator): address {
        self.metadata.sui_address
    }

    public fun total_stake_amount(self: &Validator): u64 {
        spec {
            // TODO: this should be provable rather than assumed
            assume self.staking_pool.sui_balance <= MAX_U64;
        };
        staking_pool::sui_balance(&self.staking_pool)
    }

    spec total_stake_amount {
        aborts_if false;
    }

    public fun delegate_amount(self: &Validator): u64 {
        staking_pool::sui_balance(&self.staking_pool)
    }

    /// Return the total amount staked with this validator
    public fun total_stake(self: &Validator): u64 {
        delegate_amount(self)
    }

    /// Return the voting power of this validator.
    public fun voting_power(self: &Validator): u64 {
        self.voting_power
    }

    /// Set the voting power of this validator, called only from validator_set.
    public(friend) fun set_voting_power(self: &mut Validator, new_voting_power: u64) {
        self.voting_power = new_voting_power;
    }

    public fun pending_stake_amount(self: &Validator): u64 {
        staking_pool::pending_stake_amount(&self.staking_pool)
    }

    public fun pending_principal_withdrawals(self: &Validator): u64 {
        staking_pool::pending_principal_withdrawal_amounts(&self.staking_pool)
    }

    public fun gas_price(self: &Validator): u64 {
        self.gas_price
    }

    public fun commission_rate(self: &Validator): u64 {
        self.commission_rate
    }

    public fun pool_token_exchange_rate_at_epoch(self: &Validator, epoch: u64): PoolTokenExchangeRate {
        staking_pool::pool_token_exchange_rate_at_epoch(&self.staking_pool, epoch)
    }

    public fun staking_pool_id(self: &Validator): ID {
        object::id(&self.staking_pool)
    }

    public fun is_duplicate(self: &Validator, other: &Validator): bool {
         self.metadata.sui_address == other.metadata.sui_address
            || self.metadata.name == other.metadata.name
            || self.metadata.net_address == other.metadata.net_address
            || self.metadata.p2p_address == other.metadata.p2p_address
            || self.metadata.pubkey_bytes == other.metadata.pubkey_bytes
    }

    /// Aborts if validator metadata is valid
    public fun validate_metadata(metadata: &ValidatorMetadata) {
        validate_metadata_bcs(bcs::to_bytes(metadata));
    }

    public native fun validate_metadata_bcs(metadata: vector<u8>);

    spec validate_metadata_bcs {
        pragma opaque;
        // TODO: stub to be replaced by actual abort conditions if any
        aborts_if [abstract] true;
        // TODO: specify actual function behavior
     }

    #[test_only]
    public fun get_staking_pool_ref(self: &Validator) : &StakingPool {
        &self.staking_pool
    }

    // CAUTION: THIS CODE IS ONLY FOR TESTING AND THIS MACRO MUST NEVER EVER BE REMOVED.
    // Creates a validator - bypassing the proof of possession in check in the process.
    // TODO: Refactor to share code with new().
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
        p2p_address: vector<u8>,
        consensus_address: vector<u8>,
        worker_address: vector<u8>,
        stake: Balance<SUI>,
        coin_locked_until_epoch: Option<EpochTimeLock>,
        gas_price: u64,
        commission_rate: u64,
        starting_epoch: u64,
        ctx: &mut TxContext
    ): Validator {
        assert!(
            // TODO: These constants are arbitrary, will adjust once we know more.
            vector::length(&net_address) <= 128
                && vector::length(&p2p_address) <= 128
                && vector::length(&name) <= 128
                && vector::length(&description) <= 150
                && vector::length(&pubkey_bytes) <= 128,
            0
        );
        let stake_amount = balance::value(&stake);
        let staking_pool = staking_pool::new(starting_epoch, ctx);
        // Add the validator's starting stake to the staking pool.
        staking_pool::request_add_delegation(&mut staking_pool, stake, coin_locked_until_epoch, sui_address, sui_address, starting_epoch, ctx);
        // We immediately process this delegation as they are at validator setup time and this is the validator staking with itself.
        staking_pool::process_pending_delegation(&mut staking_pool, starting_epoch);
        Validator {
            metadata: new_metadata(
                sui_address,
                pubkey_bytes,
                network_pubkey_bytes,
                worker_pubkey_bytes,
                proof_of_possession,
                string::from_ascii(ascii::string(name)),
                string::from_ascii(ascii::string(description)),
                url::new_unsafe_from_bytes(image_url),
                url::new_unsafe_from_bytes(project_url),
                net_address,
                p2p_address,
                consensus_address,
                worker_address,
            ),
            voting_power: stake_amount,
            gas_price,
            staking_pool,
            commission_rate,
            next_epoch_stake: stake_amount,
            next_epoch_delegation: 0,
            next_epoch_gas_price: gas_price,
            next_epoch_commission_rate: commission_rate,
        }
    }
}

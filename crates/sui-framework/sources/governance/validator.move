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
    use std::option::{Option, Self};
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
    friend sui::sui_system_tests;
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

    /// Invalid primary_address field in ValidatorMetadata
    const EMetadataInvalidPrimaryAddr: u64 = 6;

    /// Invalidworker_address field in ValidatorMetadata
    const EMetadataInvalidWorkerAddr: u64 = 7;

    /// Commission rate set by the validator is higher than the threshold
    const ECommissionRateTooHigh: u64 = 8;

    const EInvalidProofOfPossession: u64 = 0;

    const MAX_COMMISSION_RATE: u64 = 10_000; // Max rate is 100%, which is 10K base points

    struct ValidatorMetadata has store, drop, copy {
        /// The Sui Address of the validator. This is the sender that created the Validator object,
        /// and also the address to send validator/coins to during withdraws.
        sui_address: address,
        /// The public key bytes corresponding to the private key that the validator
        /// holds to sign transactions. For now, this is the same as AuthorityName.
        protocol_pubkey_bytes: vector<u8>,
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
        primary_address: vector<u8>,
        /// The address of the narwhal worker
        worker_address: vector<u8>,

        /// "next_epoch" metadata only takes effects in the next epoch.
        /// If none, current value will stay unchanged.
        next_epoch_protocol_pubkey_bytes: Option<vector<u8>>,
        next_epoch_proof_of_possession: Option<vector<u8>>,
        next_epoch_network_pubkey_bytes: Option<vector<u8>>,
        next_epoch_worker_pubkey_bytes: Option<vector<u8>>,
        next_epoch_net_address: Option<vector<u8>>,
        next_epoch_p2p_address: Option<vector<u8>>,
        next_epoch_primary_address: Option<vector<u8>>,
        next_epoch_worker_address: Option<vector<u8>>,
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
        /// Total amount of stake that would be active in the next epoch.
        next_epoch_stake: u64,
        /// This validator's gas price quote for the next epoch.
        next_epoch_gas_price: u64,
        /// The commission rate of the validator starting the next epoch, in basis point.
        next_epoch_commission_rate: u64,
    }

    const PROOF_OF_POSSESSION_DOMAIN: vector<u8> = vector[107, 111, 115, 107];

    fun verify_proof_of_possession(
        proof_of_possession: vector<u8>,
        sui_address: address,
        protocol_pubkey_bytes: vector<u8>
    ) {
        // The proof of possession is the signature over ValidatorPK || AccountAddress.
        // This proves that the account address is owned by the holder of ValidatorPK, and ensures
        // that PK exists.
        let signed_bytes = protocol_pubkey_bytes;
        let address_bytes = to_bytes(&sui_address);
        vector::append(&mut signed_bytes, address_bytes);
        assert!(
            bls12381_min_sig_verify_with_domain(&proof_of_possession, &protocol_pubkey_bytes, signed_bytes, PROOF_OF_POSSESSION_DOMAIN) == true,
            EInvalidProofOfPossession
        );
    }

    public(friend) fun new_metadata(
        sui_address: address,
        protocol_pubkey_bytes: vector<u8>,
        network_pubkey_bytes: vector<u8>,
        worker_pubkey_bytes: vector<u8>,
        proof_of_possession: vector<u8>,
        name: String,
        description: String,
        image_url: Url,
        project_url: Url,
        net_address: vector<u8>,
        p2p_address: vector<u8>,
        primary_address: vector<u8>,
        worker_address: vector<u8>,
    ): ValidatorMetadata {
        let metadata = ValidatorMetadata {
            sui_address,
            protocol_pubkey_bytes,
            network_pubkey_bytes,
            worker_pubkey_bytes,
            proof_of_possession,
            name,
            description,
            image_url,
            project_url,
            net_address,
            p2p_address,
            primary_address,
            worker_address,
            next_epoch_protocol_pubkey_bytes: option::none(),
            next_epoch_network_pubkey_bytes: option::none(),
            next_epoch_worker_pubkey_bytes: option::none(),
            next_epoch_proof_of_possession: option::none(),
            next_epoch_net_address: option::none(),
            next_epoch_p2p_address: option::none(),
            next_epoch_primary_address: option::none(),
            next_epoch_worker_address: option::none(),
        };
        metadata
    }

    public(friend) fun new(
        sui_address: address,
        protocol_pubkey_bytes: vector<u8>,
        network_pubkey_bytes: vector<u8>,
        worker_pubkey_bytes: vector<u8>,
        proof_of_possession: vector<u8>,
        name: vector<u8>,
        description: vector<u8>,
        image_url: vector<u8>,
        project_url: vector<u8>,
        net_address: vector<u8>,
        p2p_address: vector<u8>,
        primary_address: vector<u8>,
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
                && vector::length(&protocol_pubkey_bytes) <= 128,
            0
        );
        assert!(commission_rate <= MAX_COMMISSION_RATE, ECommissionRateTooHigh);
        verify_proof_of_possession(
            proof_of_possession,
            sui_address,
            protocol_pubkey_bytes
        );

        let metadata = new_metadata(
            sui_address,
            protocol_pubkey_bytes,
            network_pubkey_bytes,
            worker_pubkey_bytes,
            proof_of_possession,
            string::from_ascii(ascii::string(name)),
            string::from_ascii(ascii::string(description)),
            url::new_unsafe_from_bytes(image_url),
            url::new_unsafe_from_bytes(project_url),
            net_address,
            p2p_address,
            primary_address,
            worker_address,
        );

        validate_metadata(&metadata);

        new_from_metadata(
            metadata,
            stake,
            coin_locked_until_epoch,
            gas_price,
            commission_rate,
            starting_epoch,
            ctx
        )
    }

    /// Deactivate this validator's staking pool
    public(friend) fun deactivate(self: &mut Validator, deactivation_epoch: u64) {
        staking_pool::deactivate_staking_pool(&mut self.staking_pool, deactivation_epoch)
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
        self.next_epoch_stake = self.next_epoch_stake + delegate_amount;
    }

    /// Request to withdraw delegation from the validator's staking pool, processed at the end of the epoch.
    public(friend) fun request_withdraw_delegation(
        self: &mut Validator,
        staked_sui: StakedSui,
        ctx: &mut TxContext,
    ) {
        let withdraw_amount = staking_pool::request_withdraw_delegation(
                &mut self.staking_pool, staked_sui, ctx);
        self.next_epoch_stake = self.next_epoch_stake - withdraw_amount;
    }

    /// Request to set new gas price for the next epoch.
    public(friend) fun request_set_gas_price(self: &mut Validator, new_price: u64) {
        self.next_epoch_gas_price = new_price;
    }

    public(friend) fun request_set_commission_rate(self: &mut Validator, new_commission_rate: u64) {
        assert!(new_commission_rate <= MAX_COMMISSION_RATE, ECommissionRateTooHigh);
        self.next_epoch_commission_rate = new_commission_rate;
    }

    /// Deposit delegations rewards into the validator's staking pool, called at the end of the epoch.
    public(friend) fun deposit_delegation_rewards(self: &mut Validator, reward: Balance<SUI>) {
        self.next_epoch_stake = self.next_epoch_stake + balance::value(&reward);
        staking_pool::deposit_rewards(&mut self.staking_pool, reward);
    }

    /// Process pending delegations and withdraws, called at the end of the epoch.
    public(friend) fun process_pending_delegations_and_withdraws(self: &mut Validator, ctx: &mut TxContext) {
        staking_pool::process_pending_delegations_and_withdraws(&mut self.staking_pool, ctx);
        assert!(delegate_amount(self) == self.next_epoch_stake, 0);
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

    public fun name(self: &Validator): &String {
        &self.metadata.name
    }

    public fun description(self: &Validator): &String {
        &self.metadata.description
    }

    public fun image_url(self: &Validator): &Url {
        &self.metadata.image_url
    }

    public fun project_url(self: &Validator): &Url {
        &self.metadata.project_url
    }

    public fun network_address(self: &Validator): &vector<u8> {
        &self.metadata.net_address
    }

    public fun p2p_address(self: &Validator): &vector<u8> {
        &self.metadata.p2p_address
    }

    public fun primary_address(self: &Validator): &vector<u8> {
        &self.metadata.primary_address
    }

    public fun worker_address(self: &Validator): &vector<u8> {
        &self.metadata.worker_address
    }

    public fun protocol_pubkey_bytes(self: &Validator): &vector<u8> {
        &self.metadata.protocol_pubkey_bytes
    }

    public fun proof_of_possession(self: &Validator): &vector<u8> {
        &self.metadata.proof_of_possession
    }

    public fun network_pubkey_bytes(self: &Validator): &vector<u8> {
        &self.metadata.network_pubkey_bytes
    }

    public fun worker_pubkey_bytes(self: &Validator): &vector<u8> {
        &self.metadata.worker_pubkey_bytes
    }

    public fun next_epoch_network_address(self: &Validator): &Option<vector<u8>> {
        &self.metadata.next_epoch_net_address
    }

    public fun next_epoch_p2p_address(self: &Validator): &Option<vector<u8>> {
        &self.metadata.next_epoch_p2p_address
    }

    public fun next_epoch_primary_address(self: &Validator): &Option<vector<u8>> {
        &self.metadata.next_epoch_primary_address
    }

    public fun next_epoch_worker_address(self: &Validator): &Option<vector<u8>> {
        &self.metadata.next_epoch_worker_address
    }

    public fun next_epoch_protocol_pubkey_bytes(self: &Validator): &Option<vector<u8>> {
        &self.metadata.next_epoch_protocol_pubkey_bytes
    }

    public fun next_epoch_proof_of_possession(self: &Validator): &Option<vector<u8>> {
        &self.metadata.next_epoch_proof_of_possession
    }

    public fun next_epoch_network_pubkey_bytes(self: &Validator): &Option<vector<u8>> {
        &self.metadata.next_epoch_network_pubkey_bytes
    }

    public fun next_epoch_worker_pubkey_bytes(self: &Validator): &Option<vector<u8>> {
        &self.metadata.next_epoch_worker_pubkey_bytes
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

    public fun pending_stake_withdraw_amount(self: &Validator): u64 {
        staking_pool::pending_stake_withdraw_amount(&self.staking_pool)
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
            || self.metadata.protocol_pubkey_bytes == other.metadata.protocol_pubkey_bytes
    }

    // ==== Validator Metadata Management Functions ====

    /// Update name of the validator.
    public(friend) fun update_name(self: &mut Validator, name: String) {
        self.metadata.name = name;
    }

    /// Update description of the validator.
    public(friend) fun update_description(self: &mut Validator, description: String) {
        self.metadata.description = description;
    }

    /// Update image url of the validator.
    public(friend) fun update_image_url(self: &mut Validator, image_url: Url) {
        self.metadata.image_url = image_url;
    }

    /// Update project url of the validator.
    public(friend) fun update_project_url(self: &mut Validator, project_url: Url) {
        self.metadata.project_url = project_url;
    }

    /// Update network address of this validator, taking effects from next epoch
    public(friend) fun update_next_epoch_network_address(self: &mut Validator, net_address: vector<u8>) {
        self.metadata.next_epoch_net_address = option::some(net_address);
        validate_metadata(&self.metadata);
    }

    /// Update p2p address of this validator, taking effects from next epoch
    public(friend) fun update_next_epoch_p2p_address(self: &mut Validator, p2p_address: vector<u8>) {
        self.metadata.next_epoch_p2p_address = option::some(p2p_address);
        validate_metadata(&self.metadata);
    }

    /// Update consensus address of this validator, taking effects from next epoch
    public(friend) fun update_next_epoch_primary_address(self: &mut Validator, primary_address: vector<u8>) {
        self.metadata.next_epoch_primary_address = option::some(primary_address);
        validate_metadata(&self.metadata);
    }

    /// Update worker address of this validator, taking effects from next epoch
    public(friend) fun update_next_epoch_worker_address(self: &mut Validator, worker_address: vector<u8>) {
        self.metadata.next_epoch_worker_address = option::some(worker_address);
        validate_metadata(&self.metadata);
    }

    /// Update protocol public key of this validator, taking effects from next epoch
    public(friend) fun update_next_epoch_protocol_pubkey(self: &mut Validator, protocol_pubkey: vector<u8>, proof_of_possession: vector<u8>) {
        // TODO move proof of possession verification to the native function
        verify_proof_of_possession(proof_of_possession, self.metadata.sui_address, protocol_pubkey);
        self.metadata.next_epoch_protocol_pubkey_bytes = option::some(protocol_pubkey);
        self.metadata.next_epoch_proof_of_possession = option::some(proof_of_possession);
        validate_metadata(&self.metadata);
    }

    /// Update network public key of this validator, taking effects from next epoch
    public(friend) fun update_next_epoch_network_pubkey(self: &mut Validator, network_pubkey: vector<u8>) {
        self.metadata.next_epoch_network_pubkey_bytes = option::some(network_pubkey);
        validate_metadata(&self.metadata);
    }

    /// Update Narwhal worker public key of this validator, taking effects from next epoch
    public(friend) fun update_next_epoch_worker_pubkey(self: &mut Validator, worker_pubkey: vector<u8>) {
        self.metadata.next_epoch_worker_pubkey_bytes = option::some(worker_pubkey);
        validate_metadata(&self.metadata);
    }

    /// Effectutate all staged next epoch metadata for this validator.
    /// NOTE: this function SHOULD ONLY be called by validator_set when
    /// advancing an epoch.
    public(friend) fun effectuate_staged_metadata(self: &mut Validator) {
        if (option::is_some(next_epoch_network_address(self))) {
            self.metadata.net_address = option::extract(&mut self.metadata.next_epoch_net_address);
            self.metadata.next_epoch_net_address = option::none();
        };

        if (option::is_some(next_epoch_p2p_address(self))) {
            self.metadata.p2p_address = option::extract(&mut self.metadata.next_epoch_p2p_address);
            self.metadata.next_epoch_p2p_address = option::none();
        };

        if (option::is_some(next_epoch_primary_address(self))) {
            self.metadata.primary_address = option::extract(&mut self.metadata.next_epoch_primary_address);
            self.metadata.next_epoch_primary_address = option::none();
        };

        if (option::is_some(next_epoch_worker_address(self))) {
            self.metadata.worker_address = option::extract(&mut self.metadata.next_epoch_worker_address);
            self.metadata.next_epoch_worker_address = option::none();
        };

        if (option::is_some(next_epoch_protocol_pubkey_bytes(self))) {
            self.metadata.protocol_pubkey_bytes = option::extract(&mut self.metadata.next_epoch_protocol_pubkey_bytes);
            self.metadata.next_epoch_protocol_pubkey_bytes = option::none();
            self.metadata.proof_of_possession = option::extract(&mut self.metadata.next_epoch_proof_of_possession);
            self.metadata.next_epoch_proof_of_possession = option::none();
        };

        if (option::is_some(next_epoch_network_pubkey_bytes(self))) {
            self.metadata.network_pubkey_bytes = option::extract(&mut self.metadata.next_epoch_network_pubkey_bytes);
            self.metadata.next_epoch_network_pubkey_bytes = option::none();
        };

        if (option::is_some(next_epoch_worker_pubkey_bytes(self))) {
            self.metadata.worker_pubkey_bytes = option::extract(&mut self.metadata.next_epoch_worker_pubkey_bytes);
            self.metadata.next_epoch_worker_pubkey_bytes = option::none();
        };
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

    /// Create a new validator from the given `ValidatorMetadata`, called by both `new` and `new_for_testing`.
    fun new_from_metadata(
        metadata: ValidatorMetadata,
        stake: Balance<SUI>,
        coin_locked_until_epoch: Option<EpochTimeLock>,
        gas_price: u64,
        commission_rate: u64,
        starting_epoch: u64,
        ctx: &mut TxContext
    ): Validator {
        let sui_address = metadata.sui_address;
        let stake_amount = balance::value(&stake);

        let staking_pool = staking_pool::new(starting_epoch, ctx);
        // Add the validator's starting stake to the staking pool.
        staking_pool::request_add_delegation(&mut staking_pool, stake, coin_locked_until_epoch, sui_address, sui_address, starting_epoch, ctx);
        // We immediately process this delegation as they are at validator setup time and this is the validator staking with itself.
        staking_pool::process_pending_delegation(&mut staking_pool);
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
            next_epoch_gas_price: gas_price,
            next_epoch_commission_rate: commission_rate,
        }
    }

    // CAUTION: THIS CODE IS ONLY FOR TESTING AND THIS MACRO MUST NEVER EVER BE REMOVED.
    // Creates a validator - bypassing the proof of possession check and other metadata validation in the process.
    #[test_only]
    public(friend) fun new_for_testing(
        sui_address: address,
        protocol_pubkey_bytes: vector<u8>,
        network_pubkey_bytes: vector<u8>,
        worker_pubkey_bytes: vector<u8>,
        proof_of_possession: vector<u8>,
        name: vector<u8>,
        description: vector<u8>,
        image_url: vector<u8>,
        project_url: vector<u8>,
        net_address: vector<u8>,
        p2p_address: vector<u8>,
        primary_address: vector<u8>,
        worker_address: vector<u8>,
        stake: Balance<SUI>,
        coin_locked_until_epoch: Option<EpochTimeLock>,
        gas_price: u64,
        commission_rate: u64,
        starting_epoch: u64,
        ctx: &mut TxContext
    ): Validator {
        new_from_metadata(
            new_metadata(
                sui_address,
                protocol_pubkey_bytes,
                network_pubkey_bytes,
                worker_pubkey_bytes,
                proof_of_possession,
                string::from_ascii(ascii::string(name)),
                string::from_ascii(ascii::string(description)),
                url::new_unsafe_from_bytes(image_url),
                url::new_unsafe_from_bytes(project_url),
                net_address,
                p2p_address,
                primary_address,
                worker_address,
            ),
            stake,
            coin_locked_until_epoch,
            gas_price,
            commission_rate,
            starting_epoch,
            ctx
        )
    }
}

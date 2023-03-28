// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui_system::validator {
    use std::ascii;
    use std::vector;
    use std::bcs;

    use sui::balance::{Self, Balance};
    use sui::sui::SUI;
    use sui::tx_context::{Self, TxContext};
    use sui_system::validator_cap::{Self, ValidatorOperationCap};
    use sui::object::{Self, ID};
    use std::option::{Option, Self};
    use sui_system::staking_pool::{Self, PoolTokenExchangeRate, StakedSui, StakingPool};
    use std::string::{Self, String};
    use sui::url::Url;
    use sui::url;
    use sui::event;
    use sui::bag::Bag;
    use sui::bag;
    friend sui_system::genesis;
    friend sui_system::sui_system_state_inner;
    friend sui_system::validator_wrapper;
    friend sui_system::validator_set;
    friend sui_system::voting_power;

    #[test_only]
    friend sui_system::validator_tests;
    #[test_only]
    friend sui_system::validator_set_tests;
    #[test_only]
    friend sui_system::sui_system_tests;
    #[test_only]
    friend sui_system::governance_test_utils;

    /// Invalid proof_of_possession field in ValidatorMetadata
    const EInvalidProofOfPossession: u64 = 0;

    /// Invalid pubkey_bytes field in ValidatorMetadata
    const EMetadataInvalidPubkey: u64 = 1;

    /// Invalid network_pubkey_bytes field in ValidatorMetadata
    const EMetadataInvalidNetPubkey: u64 = 2;

    /// Invalid worker_pubkey_bytes field in ValidatorMetadata
    const EMetadataInvalidWorkerPubkey: u64 = 3;

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

    /// Validator Metadata is too long
    const EValidatorMetadataExceedingLengthLimit: u64 = 9;

    /// Intended validator is not a candidate one.
    const ENotValidatorCandidate: u64 = 10;

    /// Stake amount is invalid or wrong.
    const EInvalidStakeAmount: u64 = 11;

    /// Function called during non-genesis times.
    const ECalledDuringNonGenesis: u64 = 12;

    /// New Capability is not created by the validator itself
    const ENewCapNotCreatedByValidatorItself: u64 = 100;

    /// Capability code is not valid
    const EInvalidCap: u64 = 101;


    const MAX_COMMISSION_RATE: u64 = 10_000; // Max rate is 100%, which is 10K base points

    const MAX_VALIDATOR_METADATA_LENGTH: u64 = 256;

    struct ValidatorMetadata has store {
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
        net_address: String,
        /// The address of the validator used for p2p activities such as state sync (could also contain extra info such as port, DNS and etc.).
        p2p_address: String,
        /// The address of the narwhal primary
        primary_address: String,
        /// The address of the narwhal worker
        worker_address: String,

        /// "next_epoch" metadata only takes effects in the next epoch.
        /// If none, current value will stay unchanged.
        next_epoch_protocol_pubkey_bytes: Option<vector<u8>>,
        next_epoch_proof_of_possession: Option<vector<u8>>,
        next_epoch_network_pubkey_bytes: Option<vector<u8>>,
        next_epoch_worker_pubkey_bytes: Option<vector<u8>>,
        next_epoch_net_address: Option<String>,
        next_epoch_p2p_address: Option<String>,
        next_epoch_primary_address: Option<String>,
        next_epoch_worker_address: Option<String>,

        /// Any extra fields that's not defined statically.
        extra_fields: Bag,
    }

    struct Validator has store {
        /// Summary of the validator.
        metadata: ValidatorMetadata,
        /// The voting power of this validator, which might be different from its
        /// stake amount.
        voting_power: u64,
        /// The ID of this validator's current valid `UnverifiedValidatorOperationCap`
        operation_cap_id: ID,
        /// Gas price quote, updated only at end of epoch.
        gas_price: u64,
        /// Staking pool for this validator.
        staking_pool: StakingPool,
        /// Commission rate of the validator, in basis point.
        commission_rate: u64,
        /// Total amount of stake that would be active in the next epoch.
        next_epoch_stake: u64,
        /// This validator's gas price quote for the next epoch.
        next_epoch_gas_price: u64,
        /// The commission rate of the validator starting the next epoch, in basis point.
        next_epoch_commission_rate: u64,
        /// Any extra fields that's not defined statically.
        extra_fields: Bag,
    }

    /// Event emitted when a new stake request is received.
    struct StakingRequestEvent has copy, drop {
        pool_id: ID,
        validator_address: address,
        staker_address: address,
        epoch: u64,
        amount: u64,
    }

    /// Event emitted when a new unstake request is received.
    struct UnstakingRequestEvent has copy, drop {
        pool_id: ID,
        validator_address: address,
        staker_address: address,
        stake_activation_epoch: u64,
        unstaking_epoch: u64,
        principal_amount: u64,
        reward_amount: u64,
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
        net_address: String,
        p2p_address: String,
        primary_address: String,
        worker_address: String,
        extra_fields: Bag,
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
            extra_fields,
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
        gas_price: u64,
        commission_rate: u64,
        ctx: &mut TxContext
    ): Validator {
        assert!(
            vector::length(&net_address) <= MAX_VALIDATOR_METADATA_LENGTH
                && vector::length(&p2p_address) <= MAX_VALIDATOR_METADATA_LENGTH
                && vector::length(&primary_address) <= MAX_VALIDATOR_METADATA_LENGTH
                && vector::length(&worker_address) <= MAX_VALIDATOR_METADATA_LENGTH
                && vector::length(&name) <= MAX_VALIDATOR_METADATA_LENGTH
                && vector::length(&description) <= MAX_VALIDATOR_METADATA_LENGTH
                && vector::length(&image_url) <= MAX_VALIDATOR_METADATA_LENGTH
                && vector::length(&project_url) <= MAX_VALIDATOR_METADATA_LENGTH,
            EValidatorMetadataExceedingLengthLimit
        );
        assert!(commission_rate <= MAX_COMMISSION_RATE, ECommissionRateTooHigh);

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
            string::from_ascii(ascii::string(net_address)),
            string::from_ascii(ascii::string(p2p_address)),
            string::from_ascii(ascii::string(primary_address)),
            string::from_ascii(ascii::string(worker_address)),
            bag::new(ctx),
        );

        // Checks that the keys & addresses & PoP are valid.
        validate_metadata(&metadata);

        new_from_metadata(
            metadata,
            gas_price,
            commission_rate,
            ctx
        )
    }

    /// Deactivate this validator's staking pool
    public(friend) fun deactivate(self: &mut Validator, deactivation_epoch: u64) {
        staking_pool::deactivate_staking_pool(&mut self.staking_pool, deactivation_epoch)
    }

    public(friend) fun activate(self: &mut Validator, activation_epoch: u64) {
        staking_pool::activate_staking_pool(&mut self.staking_pool, activation_epoch);
    }

    /// Process pending stake and pending withdraws, and update the gas price.
    public(friend) fun adjust_stake_and_gas_price(self: &mut Validator) {
        self.gas_price = self.next_epoch_gas_price;
        self.commission_rate = self.next_epoch_commission_rate;
    }

    /// Request to add stake to the validator's staking pool, processed at the end of the epoch.
    public(friend) fun request_add_stake(
        self: &mut Validator,
        stake: Balance<SUI>,
        staker_address: address,
        ctx: &mut TxContext,
    ) {
        let stake_amount = balance::value(&stake);
        assert!(stake_amount > 0, EInvalidStakeAmount);
        let stake_epoch = tx_context::epoch(ctx) + 1;
        staking_pool::request_add_stake(
            &mut self.staking_pool, stake, staker_address, stake_epoch, ctx
        );
        // Process stake right away if staking pool is preactive.
        if (staking_pool::is_preactive(&self.staking_pool)) {
            staking_pool::process_pending_stake(&mut self.staking_pool);
        };
        self.next_epoch_stake = self.next_epoch_stake + stake_amount;
        event::emit(
            StakingRequestEvent {
                pool_id: staking_pool_id(self),
                validator_address: self.metadata.sui_address,
                staker_address,
                epoch: tx_context::epoch(ctx),
                amount: stake_amount,
            }
        );
    }

    /// Request to add stake to the validator's staking pool at genesis
    public(friend) fun request_add_stake_at_genesis(
        self: &mut Validator,
        stake: Balance<SUI>,
        staker_address: address,
        ctx: &mut TxContext,
    ) {
        assert!(tx_context::epoch(ctx) == 0, ECalledDuringNonGenesis);
        let stake_amount = balance::value(&stake);
        assert!(stake_amount > 0, EInvalidStakeAmount);

        staking_pool::request_add_stake(
            &mut self.staking_pool,
            stake,
            staker_address,
            0, // epoch 0 -- genesis
            ctx
        );

        // Process stake right away
        staking_pool::process_pending_stake(&mut self.staking_pool);
        self.next_epoch_stake = self.next_epoch_stake + stake_amount;
    }

    /// Request to withdraw stake from the validator's staking pool, processed at the end of the epoch.
    public(friend) fun request_withdraw_stake(
        self: &mut Validator,
        staked_sui: StakedSui,
        ctx: &mut TxContext,
    ) {
        let principal_amount = staking_pool::staked_sui_amount(&staked_sui);
        let stake_activation_epoch = staking_pool::stake_activation_epoch(&staked_sui);
        let withdraw_amount = staking_pool::request_withdraw_stake(
                &mut self.staking_pool, staked_sui, ctx);
        let reward_amount = withdraw_amount - principal_amount;
        self.next_epoch_stake = self.next_epoch_stake - withdraw_amount;
        event::emit(
            UnstakingRequestEvent {
                pool_id: staking_pool_id(self),
                validator_address: self.metadata.sui_address,
                staker_address: tx_context::sender(ctx),
                stake_activation_epoch,
                unstaking_epoch: tx_context::epoch(ctx),
                principal_amount,
                reward_amount,
            }
        )
    }

    /// Request to set new gas price for the next epoch.
    /// Need to present a `ValidatorOperationCap`.
    public(friend) fun request_set_gas_price(
        self: &mut Validator,
        verified_cap: ValidatorOperationCap,
        new_price: u64,
    ) {
        let validator_address = *validator_cap::verified_operation_cap_address(&verified_cap);
        assert!(validator_address == self.metadata.sui_address, EInvalidCap);
        self.next_epoch_gas_price = new_price;
    }

    /// Set new gas price for the candidate validator.
    public(friend) fun set_candidate_gas_price(
        self: &mut Validator,
        verified_cap: ValidatorOperationCap,
        new_price: u64
    ) {
        assert!(is_preactive(self), ENotValidatorCandidate);
        let validator_address = *validator_cap::verified_operation_cap_address(&verified_cap);
        assert!(validator_address == self.metadata.sui_address, EInvalidCap);
        self.next_epoch_gas_price = new_price;
        self.gas_price = new_price;
    }

    /// Request to set new commission rate for the next epoch.
    public(friend) fun request_set_commission_rate(self: &mut Validator, new_commission_rate: u64) {
        assert!(new_commission_rate <= MAX_COMMISSION_RATE, ECommissionRateTooHigh);
        self.next_epoch_commission_rate = new_commission_rate;
    }

    /// Set new commission rate for the candidate validator.
    public(friend) fun set_candidate_commission_rate(self: &mut Validator, new_commission_rate: u64) {
        assert!(is_preactive(self), ENotValidatorCandidate);
        assert!(new_commission_rate <= MAX_COMMISSION_RATE, ECommissionRateTooHigh);
        self.commission_rate = new_commission_rate;
    }

    /// Deposit stakes rewards into the validator's staking pool, called at the end of the epoch.
    public(friend) fun deposit_stake_rewards(self: &mut Validator, reward: Balance<SUI>) {
        self.next_epoch_stake = self.next_epoch_stake + balance::value(&reward);
        staking_pool::deposit_rewards(&mut self.staking_pool, reward);
    }

    /// Process pending stakes and withdraws, called at the end of the epoch.
    public(friend) fun process_pending_stakes_and_withdraws(self: &mut Validator, ctx: &mut TxContext) {
        staking_pool::process_pending_stakes_and_withdraws(&mut self.staking_pool, ctx);
        assert!(stake_amount(self) == self.next_epoch_stake, EInvalidStakeAmount);
    }

    /// Returns true if the validator is preactive.
    public fun is_preactive(self: &Validator): bool {
        staking_pool::is_preactive(&self.staking_pool)
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

    public fun network_address(self: &Validator): &String {
        &self.metadata.net_address
    }

    public fun p2p_address(self: &Validator): &String {
        &self.metadata.p2p_address
    }

    public fun primary_address(self: &Validator): &String {
        &self.metadata.primary_address
    }

    public fun worker_address(self: &Validator): &String {
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

    public fun next_epoch_network_address(self: &Validator): &Option<String> {
        &self.metadata.next_epoch_net_address
    }

    public fun next_epoch_p2p_address(self: &Validator): &Option<String> {
        &self.metadata.next_epoch_p2p_address
    }

    public fun next_epoch_primary_address(self: &Validator): &Option<String> {
        &self.metadata.next_epoch_primary_address
    }

    public fun next_epoch_worker_address(self: &Validator): &Option<String> {
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

    public fun operation_cap_id(self: &Validator): &ID {
        &self.operation_cap_id
    }

    public fun next_epoch_gas_price(self: &Validator): u64 {
        self.next_epoch_gas_price
    }

    // TODO: this and `delegate_amount` and `total_stake` all seem to return the same value?
    // two of the functions can probably be removed.
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

    public fun stake_amount(self: &Validator): u64 {
        staking_pool::sui_balance(&self.staking_pool)
    }

    /// Return the total amount staked with this validator
    public fun total_stake(self: &Validator): u64 {
        stake_amount(self)
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

    // MUSTFIX: We need to check this when updating metadata as well.
    public fun is_duplicate(self: &Validator, other: &Validator): bool {
         self.metadata.sui_address == other.metadata.sui_address
            || self.metadata.name == other.metadata.name
            // MUSTFIX: tests break when this is uncommented
            // || self.metadata.net_address == other.metadata.net_address
            // || self.metadata.p2p_address == other.metadata.p2p_address
            || self.metadata.protocol_pubkey_bytes == other.metadata.protocol_pubkey_bytes
            || self.metadata.network_pubkey_bytes == other.metadata.network_pubkey_bytes
            || self.metadata.network_pubkey_bytes == other.metadata.worker_pubkey_bytes
            || self.metadata.worker_pubkey_bytes == other.metadata.worker_pubkey_bytes
            || self.metadata.worker_pubkey_bytes == other.metadata.network_pubkey_bytes
            // All next epoch parameters.
            || is_equal_some(&self.metadata.next_epoch_net_address, &other.metadata.next_epoch_net_address)
            || is_equal_some(&self.metadata.next_epoch_p2p_address, &other.metadata.next_epoch_p2p_address)
            || is_equal_some(&self.metadata.next_epoch_protocol_pubkey_bytes, &other.metadata.next_epoch_protocol_pubkey_bytes)
            || is_equal_some(&self.metadata.next_epoch_network_pubkey_bytes, &other.metadata.next_epoch_network_pubkey_bytes)
            || is_equal_some(&self.metadata.next_epoch_network_pubkey_bytes, &other.metadata.next_epoch_worker_pubkey_bytes)
            || is_equal_some(&self.metadata.next_epoch_worker_pubkey_bytes, &other.metadata.next_epoch_worker_pubkey_bytes)
            || is_equal_some(&self.metadata.next_epoch_worker_pubkey_bytes, &other.metadata.next_epoch_network_pubkey_bytes)
            // My next epoch parameters with other current epoch parameters.
            || is_equal_some_and_value(&self.metadata.next_epoch_net_address, &other.metadata.net_address)
            || is_equal_some_and_value(&self.metadata.next_epoch_p2p_address, &other.metadata.p2p_address)
            || is_equal_some_and_value(&self.metadata.next_epoch_protocol_pubkey_bytes, &other.metadata.protocol_pubkey_bytes)
            || is_equal_some_and_value(&self.metadata.next_epoch_network_pubkey_bytes, &other.metadata.network_pubkey_bytes)
            || is_equal_some_and_value(&self.metadata.next_epoch_network_pubkey_bytes, &other.metadata.worker_pubkey_bytes)
            || is_equal_some_and_value(&self.metadata.next_epoch_worker_pubkey_bytes, &other.metadata.worker_pubkey_bytes)
            || is_equal_some_and_value(&self.metadata.next_epoch_worker_pubkey_bytes, &other.metadata.network_pubkey_bytes)
            // Other next epoch parameters with my current epoch parameters.
            || is_equal_some_and_value(&other.metadata.next_epoch_net_address, &self.metadata.net_address)
            || is_equal_some_and_value(&other.metadata.next_epoch_p2p_address, &self.metadata.p2p_address)
            || is_equal_some_and_value(&other.metadata.next_epoch_protocol_pubkey_bytes, &self.metadata.protocol_pubkey_bytes)
            || is_equal_some_and_value(&other.metadata.next_epoch_network_pubkey_bytes, &self.metadata.network_pubkey_bytes)
            || is_equal_some_and_value(&other.metadata.next_epoch_network_pubkey_bytes, &self.metadata.worker_pubkey_bytes)
            || is_equal_some_and_value(&other.metadata.next_epoch_worker_pubkey_bytes, &self.metadata.worker_pubkey_bytes)
            || is_equal_some_and_value(&other.metadata.next_epoch_worker_pubkey_bytes, &self.metadata.network_pubkey_bytes)
    }

    fun is_equal_some_and_value<T>(a: &Option<T>, b: &T): bool {
        if (option::is_none(a)) {
            false
        } else {
            option::borrow(a) == b
        }
    }

    fun is_equal_some<T>(a: &Option<T>, b: &Option<T>): bool {
        if (option::is_none(a) || option::is_none(b)) {
            false
        } else {
            option::borrow(a) == option::borrow(b)
        }
    }

    // ==== Validator Metadata Management Functions ====

    /// Create a new `UnverifiedValidatorOperationCap`, transfer to the validator,
    /// and registers it, thus revoking the previous cap's permission.
    public(friend) fun new_unverified_validator_operation_cap_and_transfer(self: &mut Validator, ctx: &mut TxContext) {
        let address = tx_context::sender(ctx);
        assert!(address == self.metadata.sui_address, ENewCapNotCreatedByValidatorItself);
        let new_id = validator_cap::new_unverified_validator_operation_cap_and_transfer(address, ctx);
        self.operation_cap_id = new_id;
    }

    /// Update name of the validator.
    public(friend) fun update_name(self: &mut Validator, name: vector<u8>) {
        assert!(
            vector::length(&name) <= MAX_VALIDATOR_METADATA_LENGTH,
            EValidatorMetadataExceedingLengthLimit
        );
        self.metadata.name = string::from_ascii(ascii::string(name));
    }

    /// Update description of the validator.
    public(friend) fun update_description(self: &mut Validator, description: vector<u8>) {
        assert!(
            vector::length(&description) <= MAX_VALIDATOR_METADATA_LENGTH,
            EValidatorMetadataExceedingLengthLimit
        );
        self.metadata.description = string::from_ascii(ascii::string(description));
    }

    /// Update image url of the validator.
    public(friend) fun update_image_url(self: &mut Validator, image_url: vector<u8>) {
        assert!(
            vector::length(&image_url) <= MAX_VALIDATOR_METADATA_LENGTH,
            EValidatorMetadataExceedingLengthLimit
        );
        self.metadata.image_url = url::new_unsafe_from_bytes(image_url);
    }

    /// Update project url of the validator.
    public(friend) fun update_project_url(self: &mut Validator, project_url: vector<u8>) {
        assert!(
            vector::length(&project_url) <= MAX_VALIDATOR_METADATA_LENGTH,
            EValidatorMetadataExceedingLengthLimit
        );
        self.metadata.project_url = url::new_unsafe_from_bytes(project_url);
    }

    /// Update network address of this validator, taking effects from next epoch
    public(friend) fun update_next_epoch_network_address(self: &mut Validator, net_address: vector<u8>) {
        assert!(
            vector::length(&net_address) <= MAX_VALIDATOR_METADATA_LENGTH,
            EValidatorMetadataExceedingLengthLimit
        );
        let net_address = string::from_ascii(ascii::string(net_address));
        self.metadata.next_epoch_net_address = option::some(net_address);
        validate_metadata(&self.metadata);
    }

    /// Update network address of this candidate validator
    public(friend) fun update_candidate_network_address(self: &mut Validator, net_address: vector<u8>) {
        assert!(is_preactive(self), ENotValidatorCandidate);
        assert!(
            vector::length(&net_address) <= MAX_VALIDATOR_METADATA_LENGTH,
            EValidatorMetadataExceedingLengthLimit
        );
        let net_address = string::from_ascii(ascii::string(net_address));
        self.metadata.net_address = net_address;
        validate_metadata(&self.metadata);
    }

    /// Update p2p address of this validator, taking effects from next epoch
    public(friend) fun update_next_epoch_p2p_address(self: &mut Validator, p2p_address: vector<u8>) {
        assert!(
            vector::length(&p2p_address) <= MAX_VALIDATOR_METADATA_LENGTH,
            EValidatorMetadataExceedingLengthLimit
        );
        let p2p_address = string::from_ascii(ascii::string(p2p_address));
        self.metadata.next_epoch_p2p_address = option::some(p2p_address);
        validate_metadata(&self.metadata);
    }

    /// Update p2p address of this candidate validator
    public(friend) fun update_candidate_p2p_address(self: &mut Validator, p2p_address: vector<u8>) {
        assert!(is_preactive(self), ENotValidatorCandidate);
        assert!(
            vector::length(&p2p_address) <= MAX_VALIDATOR_METADATA_LENGTH,
            EValidatorMetadataExceedingLengthLimit
        );
        let p2p_address = string::from_ascii(ascii::string(p2p_address));
        self.metadata.p2p_address = p2p_address;
        validate_metadata(&self.metadata);
    }

    /// Update primary address of this validator, taking effects from next epoch
    public(friend) fun update_next_epoch_primary_address(self: &mut Validator, primary_address: vector<u8>) {
        assert!(
            vector::length(&primary_address) <= MAX_VALIDATOR_METADATA_LENGTH,
            EValidatorMetadataExceedingLengthLimit
        );
        let primary_address = string::from_ascii(ascii::string(primary_address));
        self.metadata.next_epoch_primary_address = option::some(primary_address);
        validate_metadata(&self.metadata);
    }

    /// Update primary address of this candidate validator
    public(friend) fun update_candidate_primary_address(self: &mut Validator, primary_address: vector<u8>) {
        assert!(is_preactive(self), ENotValidatorCandidate);
        assert!(
            vector::length(&primary_address) <= MAX_VALIDATOR_METADATA_LENGTH,
            EValidatorMetadataExceedingLengthLimit
        );
        let primary_address = string::from_ascii(ascii::string(primary_address));
        self.metadata.primary_address = primary_address;
        validate_metadata(&self.metadata);
    }

    /// Update worker address of this validator, taking effects from next epoch
    public(friend) fun update_next_epoch_worker_address(self: &mut Validator, worker_address: vector<u8>) {
        assert!(
            vector::length(&worker_address) <= MAX_VALIDATOR_METADATA_LENGTH,
            EValidatorMetadataExceedingLengthLimit
        );
        let worker_address = string::from_ascii(ascii::string(worker_address));
        self.metadata.next_epoch_worker_address = option::some(worker_address);
        validate_metadata(&self.metadata);
    }

    /// Update worker address of this candidate validator
    public(friend) fun update_candidate_worker_address(self: &mut Validator, worker_address: vector<u8>) {
        assert!(is_preactive(self), ENotValidatorCandidate);
        assert!(
            vector::length(&worker_address) <= MAX_VALIDATOR_METADATA_LENGTH,
            EValidatorMetadataExceedingLengthLimit
        );
        let worker_address = string::from_ascii(ascii::string(worker_address));
        self.metadata.worker_address = worker_address;
        validate_metadata(&self.metadata);
    }

    /// Update protocol public key of this validator, taking effects from next epoch
    public(friend) fun update_next_epoch_protocol_pubkey(self: &mut Validator, protocol_pubkey: vector<u8>, proof_of_possession: vector<u8>) {
        self.metadata.next_epoch_protocol_pubkey_bytes = option::some(protocol_pubkey);
        self.metadata.next_epoch_proof_of_possession = option::some(proof_of_possession);
        validate_metadata(&self.metadata);
    }

    /// Update protocol public key of this candidate validator
    public(friend) fun update_candidate_protocol_pubkey(self: &mut Validator, protocol_pubkey: vector<u8>, proof_of_possession: vector<u8>) {
        assert!(is_preactive(self), ENotValidatorCandidate);
        self.metadata.protocol_pubkey_bytes = protocol_pubkey;
        self.metadata.proof_of_possession = proof_of_possession;
        validate_metadata(&self.metadata);
    }

    /// Update network public key of this validator, taking effects from next epoch
    public(friend) fun update_next_epoch_network_pubkey(self: &mut Validator, network_pubkey: vector<u8>) {
        self.metadata.next_epoch_network_pubkey_bytes = option::some(network_pubkey);
        validate_metadata(&self.metadata);
    }

    /// Update network public key of this candidate validator
    public(friend) fun update_candidate_network_pubkey(self: &mut Validator, network_pubkey: vector<u8>) {
        assert!(is_preactive(self), ENotValidatorCandidate);
        self.metadata.network_pubkey_bytes = network_pubkey;
        validate_metadata(&self.metadata);
    }

    /// Update Narwhal worker public key of this validator, taking effects from next epoch
    public(friend) fun update_next_epoch_worker_pubkey(self: &mut Validator, worker_pubkey: vector<u8>) {
        self.metadata.next_epoch_worker_pubkey_bytes = option::some(worker_pubkey);
        validate_metadata(&self.metadata);
    }

    /// Update Narwhal worker public key of this candidate validator
    public(friend) fun update_candidate_worker_pubkey(self: &mut Validator, worker_pubkey: vector<u8>) {
        assert!(is_preactive(self), ENotValidatorCandidate);
        self.metadata.worker_pubkey_bytes = worker_pubkey;
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
        gas_price: u64,
        commission_rate: u64,
        ctx: &mut TxContext
    ): Validator {
        let sui_address = metadata.sui_address;

        let staking_pool = staking_pool::new(ctx);

        let operation_cap_id = validator_cap::new_unverified_validator_operation_cap_and_transfer(sui_address, ctx);
        Validator {
            metadata,
            // Initialize the voting power to be 0.
            // At the epoch change where this validator is actually added to the
            // active validator set, the voting power will be updated accordingly.
            voting_power: 0,
            operation_cap_id,
            gas_price,
            staking_pool,
            commission_rate,
            next_epoch_stake: 0,
            next_epoch_gas_price: gas_price,
            next_epoch_commission_rate: commission_rate,
            extra_fields: bag::new(ctx),
        }
    }

    // CAUTION: THIS CODE IS ONLY FOR TESTING AND THIS MACRO MUST NEVER EVER BE REMOVED.
    // Creates a validator - bypassing the proof of possession check and other metadata
    // validation in the process.
    // Note: `proof_of_possession` MUST be a valid signature using sui_address and
    // protocol_pubkey_bytes. To produce a valid PoP, run [fn test_proof_of_possession].
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
        initial_stake_option: Option<Balance<SUI>>,
        gas_price: u64,
        commission_rate: u64,
        is_active_at_genesis: bool,
        ctx: &mut TxContext
    ): Validator {
        let validator = new_from_metadata(
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
                string::from_ascii(ascii::string(net_address)),
                string::from_ascii(ascii::string(p2p_address)),
                string::from_ascii(ascii::string(primary_address)),
                string::from_ascii(ascii::string(worker_address)),
                bag::new(ctx),
            ),
            gas_price,
            commission_rate,
            ctx
        );

        // Add the validator's starting stake to the staking pool if there exists one.
        if (option::is_some(&initial_stake_option)) {
            request_add_stake_at_genesis(
                &mut validator,
                option::extract(&mut initial_stake_option),
                sui_address, // give the stake to the validator
                ctx
            );
        };
        option::destroy_none(initial_stake_option);

        if (is_active_at_genesis) {
            activate(&mut validator, 0);
        };

        validator
    }
}

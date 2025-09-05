// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[allow(unused_const)]
module sui_system::validator;

use std::bcs;
use std::string::String;
use sui::bag::{Self, Bag};
use sui::balance::Balance;
use sui::event;
use sui::sui::SUI;
use sui::url::{Self, Url};
use sui_system::staking_pool::{
    Self,
    PoolTokenExchangeRate,
    StakedSui,
    StakingPool,
    FungibleStakedSui
};
use sui_system::validator_cap::{Self, ValidatorOperationCap};

public use fun sui_system::validator_wrapper::create_v1 as Validator.wrap_v1;

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
/// Invalid worker_address field in ValidatorMetadata
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
/// Validator trying to set gas price higher than threshold.
const EGasPriceHigherThanThreshold: u64 = 102;

// TODO: potentially move this value to onchain config.
const MAX_COMMISSION_RATE: u64 = 2_000; // Max rate is 20%, which is 2000 base points

const MAX_VALIDATOR_METADATA_LENGTH: u64 = 256;

// TODO: Move this to onchain config when we have a good way to do it.
/// Max gas price a validator can set is 100K MIST.
const MAX_VALIDATOR_GAS_PRICE: u64 = 100_000;

public struct ValidatorMetadata has store {
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

public struct Validator has store {
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
public struct StakingRequestEvent has copy, drop {
    pool_id: ID,
    validator_address: address,
    staker_address: address,
    epoch: u64,
    amount: u64,
}

/// Event emitted when a new unstake request is received.
public struct UnstakingRequestEvent has copy, drop {
    pool_id: ID,
    validator_address: address,
    staker_address: address,
    stake_activation_epoch: u64,
    unstaking_epoch: u64,
    principal_amount: u64,
    reward_amount: u64,
}

/// Event emitted when a staked SUI is converted to a fungible staked SUI.
public struct ConvertingToFungibleStakedSuiEvent has copy, drop {
    pool_id: ID,
    stake_activation_epoch: u64,
    staked_sui_principal_amount: u64,
    fungible_staked_sui_amount: u64,
}

/// Event emitted when a fungible staked SUI is redeemed.
public struct RedeemingFungibleStakedSuiEvent has copy, drop {
    pool_id: ID,
    fungible_staked_sui_amount: u64,
    sui_amount: u64,
}

public(package) fun new_metadata(
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
    ValidatorMetadata {
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
    }
}

public(package) fun new(
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
    ctx: &mut TxContext,
): Validator {
    assert!(
        net_address.length() <= MAX_VALIDATOR_METADATA_LENGTH
            && p2p_address.length() <= MAX_VALIDATOR_METADATA_LENGTH
            && primary_address.length() <= MAX_VALIDATOR_METADATA_LENGTH
            && worker_address.length() <= MAX_VALIDATOR_METADATA_LENGTH
            && name.length() <= MAX_VALIDATOR_METADATA_LENGTH
            && description.length() <= MAX_VALIDATOR_METADATA_LENGTH
            && image_url.length() <= MAX_VALIDATOR_METADATA_LENGTH
            && project_url.length() <= MAX_VALIDATOR_METADATA_LENGTH,
        EValidatorMetadataExceedingLengthLimit,
    );
    assert!(commission_rate <= MAX_COMMISSION_RATE, ECommissionRateTooHigh);
    assert!(gas_price < MAX_VALIDATOR_GAS_PRICE, EGasPriceHigherThanThreshold);

    let metadata = new_metadata(
        sui_address,
        protocol_pubkey_bytes,
        network_pubkey_bytes,
        worker_pubkey_bytes,
        proof_of_possession,
        name.to_ascii_string().to_string(),
        description.to_ascii_string().to_string(),
        url::new_unsafe_from_bytes(image_url),
        url::new_unsafe_from_bytes(project_url),
        net_address.to_ascii_string().to_string(),
        p2p_address.to_ascii_string().to_string(),
        primary_address.to_ascii_string().to_string(),
        worker_address.to_ascii_string().to_string(),
        bag::new(ctx),
    );

    // Checks that the keys & addresses & PoP are valid.
    metadata.validate();
    metadata.new_from_metadata(gas_price, commission_rate, ctx)
}

/// Mark Validator's `StakingPool` as inactive by setting the `deactivation_epoch`.
public(package) fun deactivate(self: &mut Validator, deactivation_epoch: u64) {
    self.staking_pool.deactivate_staking_pool(deactivation_epoch)
}

/// Activate Validator's `StakingPool` by setting the `activation_epoch`.
public(package) fun activate(self: &mut Validator, activation_epoch: u64) {
    self.staking_pool.activate_staking_pool(activation_epoch);
}

/// Process pending stake and pending withdraws, and update the gas price.
public(package) fun adjust_stake_and_gas_price(self: &mut Validator) {
    self.gas_price = self.next_epoch_gas_price;
    self.commission_rate = self.next_epoch_commission_rate;
}

/// Request to add stake to the validator's staking pool, processed at the end of the epoch.
public(package) fun request_add_stake(
    self: &mut Validator,
    stake: Balance<SUI>,
    staker_address: address,
    ctx: &mut TxContext,
): StakedSui {
    let stake_amount = stake.value();
    assert!(stake_amount > 0, EInvalidStakeAmount);
    let stake_epoch = ctx.epoch() + 1;
    let staked_sui = self.staking_pool.request_add_stake(stake, stake_epoch, ctx);
    // Process stake right away if staking pool is preactive.
    if (self.staking_pool.is_preactive()) {
        self.staking_pool.process_pending_stake();
    };
    self.next_epoch_stake = self.next_epoch_stake + stake_amount;
    event::emit(StakingRequestEvent {
        pool_id: self.staking_pool_id(),
        validator_address: self.metadata.sui_address,
        staker_address,
        epoch: ctx.epoch(),
        amount: stake_amount,
    });
    staked_sui
}

public(package) fun convert_to_fungible_staked_sui(
    self: &mut Validator,
    staked_sui: StakedSui,
    ctx: &mut TxContext,
): FungibleStakedSui {
    let stake_activation_epoch = staked_sui.activation_epoch();
    let staked_sui_principal_amount = staked_sui.amount();
    let fungible_staked_sui = self.staking_pool.convert_to_fungible_staked_sui(staked_sui, ctx);

    event::emit(ConvertingToFungibleStakedSuiEvent {
        pool_id: self.staking_pool_id(),
        stake_activation_epoch,
        staked_sui_principal_amount,
        fungible_staked_sui_amount: fungible_staked_sui.value(),
    });

    fungible_staked_sui
}

public(package) fun redeem_fungible_staked_sui(
    self: &mut Validator,
    fungible_staked_sui: FungibleStakedSui,
    ctx: &TxContext,
): Balance<SUI> {
    let fungible_staked_sui_amount = fungible_staked_sui.value();
    let sui = self.staking_pool.redeem_fungible_staked_sui(fungible_staked_sui, ctx);

    self.next_epoch_stake = self.next_epoch_stake - sui.value();

    event::emit(RedeemingFungibleStakedSuiEvent {
        pool_id: self.staking_pool_id(),
        fungible_staked_sui_amount,
        sui_amount: sui.value(),
    });

    sui
}

/// Request to add stake to the validator's staking pool at genesis
public(package) fun request_add_stake_at_genesis(
    self: &mut Validator,
    stake: Balance<SUI>,
    staker_address: address,
    ctx: &mut TxContext,
) {
    assert!(ctx.epoch() == 0, ECalledDuringNonGenesis);
    let stake_amount = stake.value();
    assert!(stake_amount > 0, EInvalidStakeAmount);

    // 0 = genesis epoch
    let staked_sui = self.staking_pool.request_add_stake(stake, 0, ctx);

    transfer::public_transfer(staked_sui, staker_address);

    // Process stake right away
    self.staking_pool.process_pending_stake();
    self.next_epoch_stake = self.next_epoch_stake + stake_amount;
}

/// Request to withdraw stake from the validator's staking pool, processed at the end of the epoch.
public(package) fun request_withdraw_stake(
    self: &mut Validator,
    staked_sui: StakedSui,
    ctx: &TxContext,
): Balance<SUI> {
    let principal_amount = staked_sui.amount();
    let stake_activation_epoch = staked_sui.activation_epoch();
    let withdrawn_stake = self.staking_pool.request_withdraw_stake(staked_sui, ctx);
    let withdraw_amount = withdrawn_stake.value();
    let reward_amount = withdraw_amount - principal_amount;
    self.next_epoch_stake = self.next_epoch_stake - withdraw_amount;
    event::emit(UnstakingRequestEvent {
        pool_id: self.staking_pool_id(),
        validator_address: self.metadata.sui_address,
        staker_address: ctx.sender(),
        stake_activation_epoch,
        unstaking_epoch: ctx.epoch(),
        principal_amount,
        reward_amount,
    });
    withdrawn_stake
}

/// Request to set new gas price for the next epoch.
/// Need to present a `ValidatorOperationCap`.
public(package) fun request_set_gas_price(
    self: &mut Validator,
    verified_cap: ValidatorOperationCap,
    new_price: u64,
) {
    assert!(new_price < MAX_VALIDATOR_GAS_PRICE, EGasPriceHigherThanThreshold);
    let validator_address = *verified_cap.verified_operation_cap_address();
    assert!(validator_address == self.metadata.sui_address, EInvalidCap);
    self.next_epoch_gas_price = new_price;
}

/// Set new gas price for the candidate validator.
public(package) fun set_candidate_gas_price(
    self: &mut Validator,
    verified_cap: ValidatorOperationCap,
    new_price: u64,
) {
    assert!(self.is_preactive(), ENotValidatorCandidate);
    assert!(new_price < MAX_VALIDATOR_GAS_PRICE, EGasPriceHigherThanThreshold);
    let validator_address = *verified_cap.verified_operation_cap_address();
    assert!(validator_address == self.metadata.sui_address, EInvalidCap);
    self.next_epoch_gas_price = new_price;
    self.gas_price = new_price;
}

/// Request to set new commission rate for the next epoch.
public(package) fun request_set_commission_rate(self: &mut Validator, new_commission_rate: u64) {
    assert!(new_commission_rate <= MAX_COMMISSION_RATE, ECommissionRateTooHigh);
    self.next_epoch_commission_rate = new_commission_rate;
}

/// Set new commission rate for the candidate validator.
public(package) fun set_candidate_commission_rate(self: &mut Validator, new_commission_rate: u64) {
    assert!(self.is_preactive(), ENotValidatorCandidate);
    assert!(new_commission_rate <= MAX_COMMISSION_RATE, ECommissionRateTooHigh);
    self.commission_rate = new_commission_rate;
}

/// Deposit stakes rewards into the validator's staking pool, called at the end of the epoch.
public(package) fun deposit_stake_rewards(self: &mut Validator, reward: Balance<SUI>) {
    self.next_epoch_stake = self.next_epoch_stake + reward.value();
    self.staking_pool.deposit_rewards(reward);
}

/// Process pending stakes and withdraws, called at the end of the epoch.
public(package) fun process_pending_stakes_and_withdraws(self: &mut Validator, ctx: &TxContext) {
    self.staking_pool.process_pending_stakes_and_withdraws(ctx);
    // TODO: bring this assertion back when we are ready.
    // assert!(stake_amount(self) == self.next_epoch_stake, EInvalidStakeAmount);
}

/// Returns true if the validator is preactive.
public fun is_preactive(self: &Validator): bool {
    self.staking_pool.is_preactive()
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

#[deprecated(note = b"Use `total_stake` instead")]
public fun total_stake_amount(self: &Validator): u64 {
    self.staking_pool.sui_balance()
}

#[deprecated(note = b"Use `total_stake` instead")]
public fun stake_amount(self: &Validator): u64 {
    self.staking_pool.sui_balance()
}

/// Return the total amount staked with this validator
public fun total_stake(self: &Validator): u64 {
    self.staking_pool.sui_balance()
}

/// Return the voting power of this validator.
public fun voting_power(self: &Validator): u64 {
    self.voting_power
}

/// Set the voting power of this validator, called only from validator_set.
public(package) fun set_voting_power(self: &mut Validator, new_voting_power: u64) {
    self.voting_power = new_voting_power;
}

public fun pending_stake_amount(self: &Validator): u64 {
    self.staking_pool.pending_stake_amount()
}

public fun pending_stake_withdraw_amount(self: &Validator): u64 {
    self.staking_pool.pending_stake_withdraw_amount()
}

public fun gas_price(self: &Validator): u64 {
    self.gas_price
}

public fun commission_rate(self: &Validator): u64 {
    self.commission_rate
}

public fun pool_token_exchange_rate_at_epoch(self: &Validator, epoch: u64): PoolTokenExchangeRate {
    self.staking_pool.pool_token_exchange_rate_at_epoch(epoch)
}

public fun staking_pool_id(self: &Validator): ID {
    object::id(&self.staking_pool)
}

// MUSTFIX: We need to check this when updating metadata as well.
public fun is_duplicate(self: &Validator, other: &Validator): bool {
    let self = &self.metadata;
    let other = &other.metadata;

    self.sui_address == other.sui_address
        || self.name == other.name
        || self.net_address == other.net_address
        || self.p2p_address == other.p2p_address
        || self.protocol_pubkey_bytes == other.protocol_pubkey_bytes
        || self.network_pubkey_bytes == other.network_pubkey_bytes
        || self.network_pubkey_bytes == other.worker_pubkey_bytes
        || self.worker_pubkey_bytes == other.worker_pubkey_bytes
        || self.worker_pubkey_bytes == other.network_pubkey_bytes
        // All next epoch parameters.
        || both_some_and_equal!(self.next_epoch_net_address, other.next_epoch_net_address)
        || both_some_and_equal!(self.next_epoch_p2p_address, other.next_epoch_p2p_address)
        || both_some_and_equal!(self.next_epoch_protocol_pubkey_bytes, other.next_epoch_protocol_pubkey_bytes)
        || both_some_and_equal!(self.next_epoch_network_pubkey_bytes, other.next_epoch_network_pubkey_bytes)
        || both_some_and_equal!(self.next_epoch_network_pubkey_bytes, other.next_epoch_worker_pubkey_bytes)
        || both_some_and_equal!(self.next_epoch_worker_pubkey_bytes, other.next_epoch_worker_pubkey_bytes)
        || both_some_and_equal!(self.next_epoch_worker_pubkey_bytes, other.next_epoch_network_pubkey_bytes)
        // My next epoch parameters with other current epoch parameters.
        || self.next_epoch_net_address.is_some_and!(|v| v == other.net_address)
        || self.next_epoch_p2p_address.is_some_and!(|v| v == other.p2p_address)
        || self.next_epoch_protocol_pubkey_bytes.is_some_and!(|v| v == other.protocol_pubkey_bytes)
        || self.next_epoch_network_pubkey_bytes.is_some_and!(|v| v == other.network_pubkey_bytes)
        || self.next_epoch_network_pubkey_bytes.is_some_and!(|v| v == other.worker_pubkey_bytes)
        || self.next_epoch_worker_pubkey_bytes.is_some_and!(|v| v == other.worker_pubkey_bytes)
        || self.next_epoch_worker_pubkey_bytes.is_some_and!(|v| v == other.network_pubkey_bytes)
        // Other next epoch parameters with my current epoch parameters.
        || other.next_epoch_net_address.is_some_and!(|v| v == self.net_address)
        || other.next_epoch_p2p_address.is_some_and!(|v| v == self.p2p_address)
        || other.next_epoch_protocol_pubkey_bytes.is_some_and!(|v| v == self.protocol_pubkey_bytes)
        || other.next_epoch_network_pubkey_bytes.is_some_and!(|v| v == self.network_pubkey_bytes)
        || other.next_epoch_network_pubkey_bytes.is_some_and!(|v| v == self.worker_pubkey_bytes)
        || other.next_epoch_worker_pubkey_bytes.is_some_and!(|v| v == self.worker_pubkey_bytes)
        || other.next_epoch_worker_pubkey_bytes.is_some_and!(|v| v == self.network_pubkey_bytes)
}

macro fun both_some_and_equal<$T>($a: Option<$T>, $b: Option<$T>): bool {
    let (a, b) = ($a, $b);
    a.is_some_and!(|a| b.is_some_and!(|b| a == b))
}

// ==== Validator Metadata Management Functions ====

/// Create a new `UnverifiedValidatorOperationCap`, transfer to the validator,
/// and registers it, thus revoking the previous cap's permission.
public(package) fun new_unverified_validator_operation_cap_and_transfer(
    self: &mut Validator,
    ctx: &mut TxContext,
) {
    let sender = ctx.sender();
    assert!(sender == self.metadata.sui_address, ENewCapNotCreatedByValidatorItself);
    let new_id = validator_cap::new_unverified_validator_operation_cap_and_transfer(sender, ctx);
    self.operation_cap_id = new_id;
}

/// Update name of the validator.
public(package) fun update_name(self: &mut Validator, name: vector<u8>) {
    assert!(name.length() <= MAX_VALIDATOR_METADATA_LENGTH, EValidatorMetadataExceedingLengthLimit);
    self.metadata.name = name.to_ascii_string().to_string();
}

/// Update description of the validator.
public(package) fun update_description(self: &mut Validator, description: vector<u8>) {
    assert!(
        description.length() <= MAX_VALIDATOR_METADATA_LENGTH,
        EValidatorMetadataExceedingLengthLimit,
    );
    self.metadata.description = description.to_ascii_string().to_string();
}

/// Update image url of the validator.
public(package) fun update_image_url(self: &mut Validator, image_url: vector<u8>) {
    assert!(
        image_url.length() <= MAX_VALIDATOR_METADATA_LENGTH,
        EValidatorMetadataExceedingLengthLimit,
    );
    self.metadata.image_url = url::new_unsafe_from_bytes(image_url);
}

/// Update project url of the validator.
public(package) fun update_project_url(self: &mut Validator, project_url: vector<u8>) {
    assert!(
        project_url.length() <= MAX_VALIDATOR_METADATA_LENGTH,
        EValidatorMetadataExceedingLengthLimit,
    );
    self.metadata.project_url = url::new_unsafe_from_bytes(project_url);
}

/// Update network address of this validator, taking effects from next epoch
public(package) fun update_next_epoch_network_address(
    self: &mut Validator,
    net_address: vector<u8>,
) {
    assert!(
        net_address.length() <= MAX_VALIDATOR_METADATA_LENGTH,
        EValidatorMetadataExceedingLengthLimit,
    );
    let net_address = net_address.to_ascii_string().to_string();
    self.metadata.next_epoch_net_address = option::some(net_address);
    self.metadata.validate();
}

/// Update network address of this candidate validator
public(package) fun update_candidate_network_address(
    self: &mut Validator,
    net_address: vector<u8>,
) {
    assert!(self.is_preactive(), ENotValidatorCandidate);
    assert!(
        net_address.length() <= MAX_VALIDATOR_METADATA_LENGTH,
        EValidatorMetadataExceedingLengthLimit,
    );
    let net_address = net_address.to_ascii_string().to_string();
    self.metadata.net_address = net_address;
    self.metadata.validate();
}

/// Update p2p address of this validator, taking effects from next epoch
public(package) fun update_next_epoch_p2p_address(self: &mut Validator, p2p_address: vector<u8>) {
    assert!(
        p2p_address.length() <= MAX_VALIDATOR_METADATA_LENGTH,
        EValidatorMetadataExceedingLengthLimit,
    );
    let p2p_address = p2p_address.to_ascii_string().to_string();
    self.metadata.next_epoch_p2p_address = option::some(p2p_address);
    self.metadata.validate();
}

/// Update p2p address of this candidate validator
public(package) fun update_candidate_p2p_address(self: &mut Validator, p2p_address: vector<u8>) {
    assert!(self.is_preactive(), ENotValidatorCandidate);
    assert!(
        p2p_address.length() <= MAX_VALIDATOR_METADATA_LENGTH,
        EValidatorMetadataExceedingLengthLimit,
    );
    let p2p_address = p2p_address.to_ascii_string().to_string();
    self.metadata.p2p_address = p2p_address;
    self.metadata.validate();
}

/// Update primary address of this validator, taking effects from next epoch
public(package) fun update_next_epoch_primary_address(
    self: &mut Validator,
    primary_address: vector<u8>,
) {
    assert!(
        primary_address.length() <= MAX_VALIDATOR_METADATA_LENGTH,
        EValidatorMetadataExceedingLengthLimit,
    );
    let primary_address = primary_address.to_ascii_string().to_string();
    self.metadata.next_epoch_primary_address = option::some(primary_address);
    self.metadata.validate();
}

/// Update primary address of this candidate validator
public(package) fun update_candidate_primary_address(
    self: &mut Validator,
    primary_address: vector<u8>,
) {
    assert!(self.is_preactive(), ENotValidatorCandidate);
    assert!(
        primary_address.length() <= MAX_VALIDATOR_METADATA_LENGTH,
        EValidatorMetadataExceedingLengthLimit,
    );
    let primary_address = primary_address.to_ascii_string().to_string();
    self.metadata.primary_address = primary_address;
    self.metadata.validate();
}

/// Update worker address of this validator, taking effects from next epoch
public(package) fun update_next_epoch_worker_address(
    self: &mut Validator,
    worker_address: vector<u8>,
) {
    assert!(
        worker_address.length() <= MAX_VALIDATOR_METADATA_LENGTH,
        EValidatorMetadataExceedingLengthLimit,
    );
    let worker_address = worker_address.to_ascii_string().to_string();
    self.metadata.next_epoch_worker_address = option::some(worker_address);
    self.metadata.validate();
}

/// Update worker address of this candidate validator
public(package) fun update_candidate_worker_address(
    self: &mut Validator,
    worker_address: vector<u8>,
) {
    assert!(self.is_preactive(), ENotValidatorCandidate);
    assert!(
        worker_address.length() <= MAX_VALIDATOR_METADATA_LENGTH,
        EValidatorMetadataExceedingLengthLimit,
    );
    let worker_address = worker_address.to_ascii_string().to_string();
    self.metadata.worker_address = worker_address;
    self.metadata.validate();
}

/// Update protocol public key of this validator, taking effects from next epoch
public(package) fun update_next_epoch_protocol_pubkey(
    self: &mut Validator,
    protocol_pubkey: vector<u8>,
    proof_of_possession: vector<u8>,
) {
    self.metadata.next_epoch_protocol_pubkey_bytes = option::some(protocol_pubkey);
    self.metadata.next_epoch_proof_of_possession = option::some(proof_of_possession);
    self.metadata.validate();
}

/// Update protocol public key of this candidate validator
public(package) fun update_candidate_protocol_pubkey(
    self: &mut Validator,
    protocol_pubkey: vector<u8>,
    proof_of_possession: vector<u8>,
) {
    assert!(self.is_preactive(), ENotValidatorCandidate);
    self.metadata.protocol_pubkey_bytes = protocol_pubkey;
    self.metadata.proof_of_possession = proof_of_possession;
    self.metadata.validate();
}

/// Update network public key of this validator, taking effects from next epoch
public(package) fun update_next_epoch_network_pubkey(
    self: &mut Validator,
    network_pubkey: vector<u8>,
) {
    self.metadata.next_epoch_network_pubkey_bytes = option::some(network_pubkey);
    self.metadata.validate();
}

/// Update network public key of this candidate validator
public(package) fun update_candidate_network_pubkey(
    self: &mut Validator,
    network_pubkey: vector<u8>,
) {
    assert!(self.is_preactive(), ENotValidatorCandidate);
    self.metadata.network_pubkey_bytes = network_pubkey;
    self.metadata.validate();
}

/// Update Narwhal worker public key of this validator, taking effects from next epoch
public(package) fun update_next_epoch_worker_pubkey(
    self: &mut Validator,
    worker_pubkey: vector<u8>,
) {
    self.metadata.next_epoch_worker_pubkey_bytes = option::some(worker_pubkey);
    self.metadata.validate();
}

/// Update Narwhal worker public key of this candidate validator
public(package) fun update_candidate_worker_pubkey(
    self: &mut Validator,
    worker_pubkey: vector<u8>,
) {
    assert!(self.is_preactive(), ENotValidatorCandidate);
    self.metadata.worker_pubkey_bytes = worker_pubkey;
    self.metadata.validate();
}

/// Effectutate all staged next epoch metadata for this validator.
/// NOTE: this function SHOULD ONLY be called by validator_set when
/// advancing an epoch.
public(package) fun effectuate_staged_metadata(self: &mut Validator) {
    do_extract!(&mut self.metadata.next_epoch_net_address, |v| {
        self.metadata.net_address = v
    });
    do_extract!(&mut self.metadata.next_epoch_p2p_address, |v| {
        self.metadata.p2p_address = v
    });
    do_extract!(&mut self.metadata.next_epoch_primary_address, |v| {
        self.metadata.primary_address = v
    });
    do_extract!(&mut self.metadata.next_epoch_worker_address, |v| {
        self.metadata.worker_address = v
    });
    do_extract!(&mut self.metadata.next_epoch_protocol_pubkey_bytes, |v| {
        self.metadata.protocol_pubkey_bytes = v;
        self.metadata.proof_of_possession = self.metadata.next_epoch_proof_of_possession.extract();
    });
    do_extract!(&mut self.metadata.next_epoch_network_pubkey_bytes, |v| {
        self.metadata.network_pubkey_bytes = v
    });
    do_extract!(&mut self.metadata.next_epoch_worker_pubkey_bytes, |v| {
        self.metadata.worker_pubkey_bytes = v
    });
}

/// Helper macro which extracts the value from `Some` and applies `$f` to it.
macro fun do_extract<$T>($o: &mut Option<$T>, $f: |$T|) {
    let o = $o;
    if (o.is_some()) {
        $f(o.extract());
    }
}

public use fun validate_metadata as ValidatorMetadata.validate;

/// Aborts if validator metadata is valid
public fun validate_metadata(metadata: &ValidatorMetadata) {
    validate_metadata_bcs(bcs::to_bytes(metadata));
}

public native fun validate_metadata_bcs(metadata: vector<u8>);

public(package) fun get_staking_pool_ref(self: &Validator): &StakingPool {
    &self.staking_pool
}

/// Create a new validator from the given `ValidatorMetadata`, called by both `new` and `new_for_testing`.
fun new_from_metadata(
    metadata: ValidatorMetadata,
    gas_price: u64,
    commission_rate: u64,
    ctx: &mut TxContext,
): Validator {
    let sui_address = metadata.sui_address;
    let staking_pool = staking_pool::new(ctx);
    let operation_cap_id = validator_cap::new_unverified_validator_operation_cap_and_transfer(
        sui_address,
        ctx,
    );

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
public(package) fun new_for_testing(
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
    ctx: &mut TxContext,
): Validator {
    let mut validator = new_from_metadata(
        new_metadata(
            sui_address,
            protocol_pubkey_bytes,
            network_pubkey_bytes,
            worker_pubkey_bytes,
            proof_of_possession,
            name.to_ascii_string().to_string(),
            description.to_ascii_string().to_string(),
            url::new_unsafe_from_bytes(image_url),
            url::new_unsafe_from_bytes(project_url),
            net_address.to_ascii_string().to_string(),
            p2p_address.to_ascii_string().to_string(),
            primary_address.to_ascii_string().to_string(),
            worker_address.to_ascii_string().to_string(),
            bag::new(ctx),
        ),
        gas_price,
        commission_rate,
        ctx,
    );

    // Add the validator's starting stake to the staking pool if there exists one.
    initial_stake_option.do!(|balance| {
        request_add_stake_at_genesis(
            &mut validator,
            balance,
            sui_address, // give the stake to the validator
            ctx,
        );
    });

    if (is_active_at_genesis) {
        validator.activate(0);
    };

    validator
}

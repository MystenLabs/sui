// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui_system::genesis;

use sui::balance::{Self, Balance};
use sui::sui::SUI;
use sui_system::stake_subsidy;
use sui_system::sui_system;
use sui_system::sui_system_state_inner;
use sui_system::validator::{Self, Validator};
use sui_system::validator_set;

public struct GenesisValidatorMetadata has copy, drop {
    name: vector<u8>,
    description: vector<u8>,
    image_url: vector<u8>,
    project_url: vector<u8>,
    sui_address: address,
    gas_price: u64,
    commission_rate: u64,
    protocol_public_key: vector<u8>,
    proof_of_possession: vector<u8>,
    network_public_key: vector<u8>,
    worker_public_key: vector<u8>,
    network_address: vector<u8>,
    p2p_address: vector<u8>,
    primary_address: vector<u8>,
    worker_address: vector<u8>,
}

public struct GenesisChainParameters has copy, drop {
    protocol_version: u64,
    chain_start_timestamp_ms: u64,
    epoch_duration_ms: u64,
    /// Stake Subsidy parameters
    stake_subsidy_start_epoch: u64,
    stake_subsidy_initial_distribution_amount: u64,
    stake_subsidy_period_length: u64,
    stake_subsidy_decrease_rate: u16,
    /// Validator committee parameters
    max_validator_count: u64,
    min_validator_joining_stake: u64,
    validator_low_stake_threshold: u64,
    validator_very_low_stake_threshold: u64,
    validator_low_stake_grace_period: u64,
}

public struct TokenDistributionSchedule {
    stake_subsidy_fund_mist: u64,
    allocations: vector<TokenAllocation>,
}

public struct TokenAllocation {
    recipient_address: address,
    amount_mist: u64,
    /// Indicates if this allocation should be staked at genesis and with which validator
    staked_with_validator: Option<address>,
}

// Error codes
/// The `create` function was called at a non-genesis epoch.
const ENotCalledAtGenesis: u64 = 0;
/// The `create` function was called with duplicate validators.
const EDuplicateValidator: u64 = 1;

#[allow(unused_function)]
/// This function will be explicitly called once at genesis.
/// It will create a singleton SuiSystemState object, which contains
/// all the information we need in the system.
fun create(
    sui_system_state_id: UID,
    mut sui_supply: Balance<SUI>,
    genesis_chain_parameters: GenesisChainParameters,
    genesis_validators: vector<GenesisValidatorMetadata>,
    token_distribution_schedule: TokenDistributionSchedule,
    ctx: &mut TxContext,
) {
    // Ensure this is only called at genesis
    assert!(ctx.epoch() == 0, ENotCalledAtGenesis);

    // Create all the `Validator` structs
    let mut validators = vector[];
    genesis_validators.do!(|genesis_validator| {
        let GenesisValidatorMetadata {
            name,
            description,
            image_url,
            project_url,
            sui_address,
            gas_price,
            commission_rate,
            protocol_public_key,
            proof_of_possession,
            network_public_key,
            worker_public_key,
            network_address,
            p2p_address,
            primary_address,
            worker_address,
        } = genesis_validator;

        let validator = validator::new(
            sui_address,
            protocol_public_key,
            network_public_key,
            worker_public_key,
            proof_of_possession,
            name,
            description,
            image_url,
            project_url,
            network_address,
            p2p_address,
            primary_address,
            worker_address,
            gas_price,
            commission_rate,
            ctx,
        );

        // Ensure that each validator is unique
        assert!(
            !validator_set::is_duplicate_validator(&validators, &validator),
            EDuplicateValidator,
        );

        validators.push_back(validator);
    });

    let TokenDistributionSchedule {
        stake_subsidy_fund_mist,
        allocations,
    } = token_distribution_schedule;

    let subsidy_fund = sui_supply.split(stake_subsidy_fund_mist);
    let storage_fund = balance::zero();

    // Allocate tokens and staking operations
    allocate_tokens(sui_supply, allocations, &mut validators, ctx);

    // Activate all validators
    validators.do_mut!(|validator| validator.activate(0));

    let system_parameters = sui_system_state_inner::create_system_parameters(
        genesis_chain_parameters.epoch_duration_ms,
        genesis_chain_parameters.stake_subsidy_start_epoch,
        // Validator committee parameters
        genesis_chain_parameters.max_validator_count,
        genesis_chain_parameters.min_validator_joining_stake,
        genesis_chain_parameters.validator_low_stake_threshold,
        genesis_chain_parameters.validator_very_low_stake_threshold,
        genesis_chain_parameters.validator_low_stake_grace_period,
        ctx,
    );

    let stake_subsidy = stake_subsidy::create(
        subsidy_fund,
        genesis_chain_parameters.stake_subsidy_initial_distribution_amount,
        genesis_chain_parameters.stake_subsidy_period_length,
        genesis_chain_parameters.stake_subsidy_decrease_rate,
        ctx,
    );

    sui_system::create(
        sui_system_state_id,
        validators,
        storage_fund,
        genesis_chain_parameters.protocol_version,
        genesis_chain_parameters.chain_start_timestamp_ms,
        system_parameters,
        stake_subsidy,
        ctx,
    );
}

fun allocate_tokens(
    mut sui_supply: Balance<SUI>,
    allocations: vector<TokenAllocation>,
    validators: &mut vector<Validator>,
    ctx: &mut TxContext,
) {
    allocations.destroy!(
        |TokenAllocation { recipient_address, amount_mist, staked_with_validator }| {
            let allocation_balance = sui_supply.split(amount_mist);
            if (staked_with_validator.is_some()) {
                let validator_address = staked_with_validator.destroy_some();
                let validator = validator_set::get_validator_mut(validators, validator_address);

                validator.request_add_stake_at_genesis(
                    allocation_balance,
                    recipient_address,
                    ctx,
                );
            } else {
                transfer::public_transfer(allocation_balance.into_coin(ctx), recipient_address);
            };
        },
    );

    // should be none left at this point.
    // Provided allocations must fully allocate the sui_supply and there
    sui_supply.destroy_zero();
}

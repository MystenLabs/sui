// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::genesis {
    use std::vector;

    use sui::balance::{Self, Balance};
    use sui::object::UID;
    use sui::sui::SUI;
    use sui::sui_system;
    use sui::tx_context::{Self, TxContext};
    use sui::validator;
    use std::option::Option;

    struct GenesisValidatorMetadata has drop, copy {
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

    struct GenesisChainParameters has drop, copy {
        protocol_version: u64,
        system_state_version: u64,
        governance_start_epoch: u64,
        chain_start_timestamp_ms: u64,
        epoch_duration_ms: u64,
        initial_stake_subsidy_distribution_amount: u64,
        stake_subsidy_period_length: u64,
        stake_subsidy_decrease_rate: u16,
    }

    struct TokenDistributionSchedule {
        stake_subsidy_fund_mist: u64,
        allocations: vector<TokenAllocation>,
    }

    struct TokenAllocation has drop {
        recipient_address: address,
        amount_mist: u64,

        /// Indicates if this allocation should be staked at genesis and with which validator
        staked_with_validator: Option<address>,
    }

    /// This function will be explicitly called once at genesis.
    /// It will create a singleton SuiSystemState object, which contains
    /// all the information we need in the system.
    fun create(
        sui_system_state_id: UID,
        sui_supply: Balance<SUI>,
        genesis_chain_parameters: GenesisChainParameters,
        genesis_validators: vector<GenesisValidatorMetadata>,
        token_distribution_schedule: TokenDistributionSchedule,
        ctx: &mut TxContext,
    ) {
        // Ensure this is only called at genesis
        assert!(tx_context::epoch(ctx) == 0, 0);

        let TokenDistributionSchedule {
            stake_subsidy_fund_mist,
            allocations: _, // Ignored in the mock test.
        } = token_distribution_schedule;

        let subsidy_fund = balance::split(
            &mut sui_supply,
            stake_subsidy_fund_mist,
        );

        // Create all the `Validator` structs
        let validators = vector::empty();
        let count = vector::length(&genesis_validators);
        let i = 0;
        while (i < count) {
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
            } = *vector::borrow(&genesis_validators, i);

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
                balance::split(&mut subsidy_fund, 10000),
                ctx
            );

            vector::push_back(&mut validators, validator);

            i = i + 1;
        };

        sui_system::create(
            sui_system_state_id,
            validators,
            subsidy_fund,
            sui_supply,     // storage_fund
            genesis_chain_parameters.protocol_version,
            genesis_chain_parameters.system_state_version,
            genesis_chain_parameters.governance_start_epoch,
            genesis_chain_parameters.chain_start_timestamp_ms,
            genesis_chain_parameters.epoch_duration_ms,
            genesis_chain_parameters.initial_stake_subsidy_distribution_amount,
            genesis_chain_parameters.stake_subsidy_period_length,
            genesis_chain_parameters.stake_subsidy_decrease_rate,
            ctx,
        );
    }
}

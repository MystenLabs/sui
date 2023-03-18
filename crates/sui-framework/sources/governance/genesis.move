// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::genesis {
    use std::vector;

    use sui::balance::{Balance, Self};
    use sui::coin;
    use sui::clock;
    use sui::sui::{Self, SUI};
    use sui::sui_system;
    use sui::tx_context::{Self, TxContext};
    use sui::validator::{Self, Validator};
    use sui::validator_set;
    use std::option::{Option, Self};

    /// Stake subisidy to be given out in the very first epoch in Mist (1 million * 10^9).
    const INIT_STAKE_SUBSIDY_AMOUNT: u64 = 1_000_000_000_000_000;

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
        governance_start_epoch: u64,
        chain_start_timestamp_ms: u64,
        epoch_duration_ms: u64,
    }

    struct TokenDistributionSchedule has drop, copy {
        stake_subsidy_fund_mist: u64,
        allocations: vector<TokenAllocation>,
    }

    struct TokenAllocation has drop, copy {
        recipient_address: address,
        amount_mist: u64,

        /// Indicates if this allocation should be staked at genesis and with which validator
        staked_with_validator: Option<address>,
    }

    /// This function will be explicitly called once at genesis.
    /// It will create a singleton SuiSystemState object, which contains
    /// all the information we need in the system.
    fun create(
        genesis_chain_parameters: GenesisChainParameters,
        genesis_validators: vector<GenesisValidatorMetadata>,
        token_distribution_schedule: TokenDistributionSchedule,
        protocol_version: u64,
        system_state_version: u64,
        ctx: &mut TxContext,
    ) {
        // Ensure this is only called at genesis
        assert!(tx_context::epoch(ctx) == 0, 0);

        let sui_supply = sui::new(ctx);
        let subsidy_fund = balance::split(
            &mut sui_supply,
            token_distribution_schedule.stake_subsidy_fund_mist
        );
        let storage_fund = balance::zero();

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
                ctx
            );

            // Ensure that each validator is unique
            assert!(
                !validator_set::is_duplicate_validator(&validators, &validator),
                2,
            );

            vector::push_back(&mut validators, validator);

            i = i + 1;
        };

        // Allocate tokens and staking operations
        allocate_tokens(
            sui_supply,
            token_distribution_schedule.allocations,
            &mut validators,
            ctx
        );

        // Activate all validators
        activate_validators(&mut validators);

        sui_system::create(
            validators,
            subsidy_fund,
            storage_fund,
            genesis_chain_parameters.governance_start_epoch,
            INIT_STAKE_SUBSIDY_AMOUNT,
            protocol_version,
            system_state_version,
            genesis_chain_parameters.chain_start_timestamp_ms,
            genesis_chain_parameters.epoch_duration_ms,
            ctx,
        );

        clock::create();
    }

    fun allocate_tokens(
        sui_supply: Balance<SUI>,
        allocations: vector<TokenAllocation>,
        validators: &mut vector<Validator>,
        ctx: &mut TxContext,
    ) {
        let count = vector::length(&allocations);
        let i = 0;
        while (i < count) {
            let allocation = *vector::borrow(&allocations, i);

            let allocation_balance = balance::split(&mut sui_supply, allocation.amount_mist);

            if (option::is_some(&allocation.staked_with_validator)) {
                let validator_address = option::destroy_some(allocation.staked_with_validator);
                let validator = validator_set::get_validator_mut(validators, validator_address);
                validator::request_add_stake_at_genesis(
                    validator,
                    allocation_balance,
                    allocation.recipient_address,
                    ctx
                );
            } else {
                sui::transfer(
                    coin::from_balance(allocation_balance, ctx),
                    allocation.recipient_address,
                );
            };

            i = i + 1;
        };

        // Provided allocations must fully allocate the sui_supply and there
        // should be none left at this point.
        balance::destroy_zero(sui_supply);
    }

    fun activate_validators(validators: &mut vector<Validator>) {
        // Activate all genesis validators
        let count = vector::length(validators);
        let i = 0;
        while (i < count) {
            let validator = vector::borrow_mut(validators, i);
            validator::activate(validator, 0);

            i = i + 1;
        };

    }
}

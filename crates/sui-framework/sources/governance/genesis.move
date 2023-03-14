// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::genesis {
    use std::vector;

    use sui::balance;
    use sui::coin;
    use sui::clock;
    use sui::sui;
    use sui::sui_system;
    use sui::tx_context::TxContext;
    use sui::validator;
    use std::option;

    /// Stake subisidy to be given out in the very first epoch in Mist (1 million * 10^9).
    const INIT_STAKE_SUBSIDY_AMOUNT: u64 = 1_000_000_000_000_000;

    /// The initial balance of the Subsidy fund in Mist (1 Billion * 10^9)
    const INIT_STAKE_SUBSIDY_FUND_BALANCE: u64 = 1_000_000_000_000_000_000;

    const INIT_STAKE_SUBSIDY_FUND_BALANCE_TEST_ONLY: u64 = 100_000_000_000_000_000;

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
        initial_sui_custody_account_address: address,
        initial_validator_stake_mist: u64,
        governance_start_epoch: u64,
        chain_start_timestamp_ms: u64,
        epoch_duration_ms: u64,
    }

    /// This function will be explicitly called once at genesis.
    /// It will create a singleton SuiSystemState object, which contains
    /// all the information we need in the system.
    fun create(
        genesis_chain_parameters: GenesisChainParameters,
        genesis_validators: vector<GenesisValidatorMetadata>,
        protocol_version: u64,
        system_state_version: u64,
        ctx: &mut TxContext,
    ) {
        let sui_supply = sui::new(ctx);
        let subsidy_fund = balance::split(&mut sui_supply, INIT_STAKE_SUBSIDY_FUND_BALANCE_TEST_ONLY);
        let storage_fund = balance::zero();
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
                // Initialize all validators with uniform stake taken from the subsidy fund.
                // TODO: change this back to take from subsidy fund instead.
                option::some(balance::split(&mut sui_supply, genesis_chain_parameters.initial_validator_stake_mist)),
                gas_price,
                commission_rate,
                ctx
            );

            validator::activate(&mut validator, 0);

            vector::push_back(&mut validators, validator);

            i = i + 1;
        };

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

        // Transfer the remaining balance of sui's supply to the initial account
        sui::transfer(coin::from_balance(sui_supply, ctx), genesis_chain_parameters.initial_sui_custody_account_address);
    }
}

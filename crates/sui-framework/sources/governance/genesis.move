// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::genesis {
    use std::vector;

    use sui::balance;
    use sui::sui;
    use sui::sui_system;
    use sui::tx_context::TxContext;
    use sui::validator;
    use std::option;

    /// The initial amount of SUI locked in the storage fund.
    const INIT_STORAGE_FUND: u64 = 1;

    /// Initial value of the lower-bound on the amount of stake required to become a validator.
    /// TODO: testnet only. Needs to be changed.
    const INIT_MIN_VALIDATOR_STAKE: u64 = 1;

    /// Initial value of the upper-bound on the number of validators.
    const INIT_MAX_VALIDATOR_COUNT: u64 = 100;

    /// Stake subisidy to be given out in the very first epoch. Placeholder value.
    const INIT_STAKE_SUBSIDY_AMOUNT: u64 = 1000000;

    /// This function will be explicitly called once at genesis.
    /// It will create a singleton SuiSystemState object, which contains
    /// all the information we need in the system.
    fun create(
        validator_pubkeys: vector<vector<u8>>,
        validator_network_pubkeys: vector<vector<u8>>,
        validator_worker_pubkeys: vector<vector<u8>>,
        validator_proof_of_possessions: vector<vector<u8>>,
        validator_sui_addresses: vector<address>,
        validator_names: vector<vector<u8>>,
        validator_descriptions: vector<vector<u8>>,
        validator_image_urls: vector<vector<u8>>,
        validator_project_urls: vector<vector<u8>>,
        validator_net_addresses: vector<vector<u8>>,
        validator_consensus_addresses: vector<vector<u8>>,
        validator_worker_addresses: vector<vector<u8>>,
        validator_stakes: vector<u64>,
        validator_gas_prices: vector<u64>,
        validator_commission_rates: vector<u64>,
        ctx: &mut TxContext,
    ) {
        let sui_supply = sui::new(ctx);
        let storage_fund = balance::increase_supply(&mut sui_supply, INIT_STORAGE_FUND);
        let validators = vector::empty();
        let count = vector::length(&validator_pubkeys);
        assert!(
            vector::length(&validator_sui_addresses) == count
                && vector::length(&validator_stakes) == count
                && vector::length(&validator_names) == count
                && vector::length(&validator_descriptions) == count
                && vector::length(&validator_image_urls) == count
                && vector::length(&validator_project_urls) == count
                && vector::length(&validator_net_addresses) == count
                && vector::length(&validator_consensus_addresses) == count
                && vector::length(&validator_worker_addresses) == count
                && vector::length(&validator_gas_prices) == count
                && vector::length(&validator_commission_rates) == count,
            1
        );
        let i = 0;
        while (i < count) {
            let sui_address = *vector::borrow(&validator_sui_addresses, i);
            let pubkey = *vector::borrow(&validator_pubkeys, i);
            let network_pubkey = *vector::borrow(&validator_network_pubkeys, i);
            let worker_pubkey = *vector::borrow(&validator_worker_pubkeys, i);
            let proof_of_possession = *vector::borrow(&validator_proof_of_possessions, i);
            let name = *vector::borrow(&validator_names, i);
            let description = *vector::borrow(&validator_descriptions, i);
            let image_url = *vector::borrow(&validator_image_urls, i);
            let project_url = *vector::borrow(&validator_project_urls, i);
            let net_address = *vector::borrow(&validator_net_addresses, i);
            let consensus_address = *vector::borrow(&validator_consensus_addresses, i);
            let worker_address = *vector::borrow(&validator_worker_addresses, i);
            let stake = *vector::borrow(&validator_stakes, i);
            let gas_price = *vector::borrow(&validator_gas_prices, i);
            let commission_rate = *vector::borrow(&validator_commission_rates, i);
            vector::push_back(&mut validators, validator::new(
                sui_address,
                pubkey,
                network_pubkey,
                worker_pubkey,
                proof_of_possession,
                name,
                description,
                image_url,
                project_url,
                net_address,
                consensus_address,
                worker_address,
                balance::increase_supply(&mut sui_supply, stake),
                option::none(),
                gas_price,
                commission_rate,
                ctx
            ));
            i = i + 1;
        };
        sui_system::create(
            validators,
            sui_supply,
            storage_fund,
            INIT_MAX_VALIDATOR_COUNT,
            INIT_MIN_VALIDATOR_STAKE,
            INIT_STAKE_SUBSIDY_AMOUNT,
        );
    }

    #[test_only]
    public fun create_for_testing(ctx: &mut TxContext) {
        let validators = vector[];
        let sui_supply = sui::new(ctx);
        let storage_fund = balance::increase_supply(&mut sui_supply, INIT_STORAGE_FUND);

        sui_system::create(
            validators,
            sui_supply,
            storage_fund,
            INIT_MAX_VALIDATOR_COUNT,
            INIT_MIN_VALIDATOR_STAKE,
            INIT_STAKE_SUBSIDY_AMOUNT
        )
    }
}

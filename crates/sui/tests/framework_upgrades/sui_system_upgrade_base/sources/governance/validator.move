// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::validator {
    use std::ascii;

    use sui::tx_context::TxContext;
    use sui::object::{ID, UID};
    use std::option::{Option, Self};
    use std::string::{Self, String};
    use sui::url::Url;
    use sui::url;
    use sui::bag::Bag;
    use sui::bag;
    use sui::balance::Balance;
    use sui::sui::SUI;
    use sui::balance;
    use sui::object;
    use sui::table;
    use sui::tx_context;
    use sui::transfer;
    use sui::table::Table;
    friend sui::genesis;
    friend sui::sui_system_state_inner;
    friend sui::validator_wrapper;

    /// A staking pool embedded in each validator struct in the system state object.
    struct StakingPool has key, store {
        id: UID,
        /// The epoch at which this pool became active.
        /// The value is `None` if the pool is pre-active and `Some(<epoch_number>)` if active or inactive.
        activation_epoch: Option<u64>,
        /// The epoch at which this staking pool ceased to be active. `None` = {pre-active, active},
        /// `Some(<epoch_number>)` if in-active, and it was de-activated at epoch `<epoch_number>`.
        deactivation_epoch: Option<u64>,
        /// The total number of SUI tokens in this pool, including the SUI in the rewards_pool, as well as in all the principal
        /// in the `StakedSui` object, updated at epoch boundaries.
        sui_balance: u64,
        /// The epoch stake rewards will be added here at the end of each epoch.
        rewards_pool: Balance<SUI>,
        /// Total number of pool tokens issued by the pool.
        pool_token_balance: u64,
        /// Exchange rate history of previous epochs. Key is the epoch number.
        /// The entries start from the `activation_epoch` of this pool and contains exchange rates at the beginning of each epoch,
        /// i.e., right after the rewards for the previous epoch have been deposited into the pool.
        exchange_rates: Table<u64, PoolTokenExchangeRate>,
        /// Pending stake amount for this epoch, emptied at epoch boundaries.
        pending_stake: u64,
        /// Pending stake withdrawn during the current epoch, emptied at epoch boundaries.
        /// This includes both the principal and rewards SUI withdrawn.
        pending_total_sui_withdraw: u64,
        /// Pending pool token withdrawn during the current epoch, emptied at epoch boundaries.
        pending_pool_token_withdraw: u64,
        /// Any extra fields that's not defined statically.
        extra_fields: Bag,
    }

    /// Struct representing the exchange rate of the stake pool token to SUI.
    struct PoolTokenExchangeRate has store, copy, drop {
        sui_amount: u64,
        pool_token_amount: u64,
    }

    /// A self-custodial object holding the staked SUI tokens.
    struct StakedSui has key {
        id: UID,
        /// ID of the staking pool we are staking with.
        pool_id: ID,
        // TODO: keeping this field here because the apps depend on it. consider removing it.
        validator_address: address,
        /// The epoch at which the stake becomes active.
        stake_activation_epoch: u64,
        /// The staked SUI tokens.
        principal: Balance<SUI>,
    }

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
        init_stake: Balance<SUI>,
        ctx: &mut TxContext
    ): Validator {
        let metadata = ValidatorMetadata {
            sui_address,
            protocol_pubkey_bytes,
            network_pubkey_bytes,
            worker_pubkey_bytes,
            proof_of_possession,
            name: string::from_ascii(ascii::string(name)),
            description: string::from_ascii(ascii::string(description)),
            image_url: url::new_unsafe_from_bytes(image_url),
            project_url: url::new_unsafe_from_bytes(project_url),
            net_address: string::from_ascii(ascii::string(net_address)),
            p2p_address: string::from_ascii(ascii::string(p2p_address)),
            primary_address: string::from_ascii(ascii::string(primary_address)),
            worker_address: string::from_ascii(ascii::string(worker_address)),
            next_epoch_protocol_pubkey_bytes: option::none(),
            next_epoch_network_pubkey_bytes: option::none(),
            next_epoch_worker_pubkey_bytes: option::none(),
            next_epoch_proof_of_possession: option::none(),
            next_epoch_net_address: option::none(),
            next_epoch_p2p_address: option::none(),
            next_epoch_primary_address: option::none(),
            next_epoch_worker_address: option::none(),
            extra_fields: bag::new(ctx),
        };

        let dummy_cap = object::new(ctx);
        let dummy_id = object::uid_to_inner(&dummy_cap);
        object::delete(dummy_cap);
        Validator {
            metadata,
            // Initialize the voting power to be 0.
            // At the epoch change where this validator is actually added to the
            // active validator set, the voting power will be updated accordingly.
            voting_power: balance::value(&init_stake),
            operation_cap_id: dummy_id,
            gas_price,
            staking_pool: new_staking_pool(init_stake, ctx),
            commission_rate,
            next_epoch_stake: 0,
            next_epoch_gas_price: gas_price,
            next_epoch_commission_rate: commission_rate,
            extra_fields: bag::new(ctx),
        }
    }

    fun new_staking_pool(init_stake: Balance<SUI>, ctx: &mut TxContext) : StakingPool {
        let exchange_rates = table::new(ctx);
        let sui_amount = balance::value(&init_stake);
        let pool = StakingPool {
            id: object::new(ctx),
            activation_epoch: option::some(tx_context::epoch(ctx)),
            deactivation_epoch: option::none(),
            sui_balance: sui_amount,
            rewards_pool: balance::zero(),
            pool_token_balance: 0,
            exchange_rates,
            pending_stake: 0,
            pending_total_sui_withdraw: 0,
            pending_pool_token_withdraw: 0,
            extra_fields: bag::new(ctx),
        };
        // We don't care about who owns the staked sui in the mock test.
        let staked_sui = StakedSui {
            id: object::new(ctx),
            pool_id: object::id(&pool),
            validator_address: tx_context::sender(ctx),
            stake_activation_epoch: tx_context::epoch(ctx),
            principal: init_stake,
        };
        transfer::transfer(staked_sui, tx_context::sender(ctx));
        pool
    }
}

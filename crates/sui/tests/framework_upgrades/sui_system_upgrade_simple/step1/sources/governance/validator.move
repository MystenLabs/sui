// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::validator {
    use std::ascii;

    use sui::tx_context::TxContext;
    use sui::object::ID;
    use std::option::{Option, Self};
    use sui::staking_pool::{Self, StakingPool};
    use std::string::{Self, String};
    use sui::url::Url;
    use sui::url;
    use sui::bag::Bag;
    use sui::bag;
    use sui::balance::Balance;
    use sui::sui::SUI;
    use sui::balance;
    use sui::object;
    friend sui::genesis;
    friend sui::sui_system_state_inner;
    friend sui::validator_wrapper;
    friend sui::validator_set;

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
            staking_pool: staking_pool::new(init_stake, ctx),
            commission_rate,
            next_epoch_stake: 0,
            next_epoch_gas_price: gas_price,
            next_epoch_commission_rate: commission_rate,
            extra_fields: bag::new(ctx),
        }
    }
}

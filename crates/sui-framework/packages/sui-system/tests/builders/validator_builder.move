// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
#[allow(unused_const)]
/// Validator Builder is a helper module which implements a builder pattern for
/// creating a `Validator` or `ValidatorMetadata` struct.
///
/// It can be used in the `TestRunner` module to set up validators in the system.
module sui_system::validator_builder;

use sui::bag;
use sui::balance;
use sui::sui::SUI;
use sui::url;
use sui_system::validator::{Self, Validator, ValidatorMetadata};

// === Constants ===

const VALID_NET_PUBKEY: vector<u8> = vector[
    171, 2, 39, 3, 139, 105, 166, 171, 153, 151, 102, 197, 151, 186, 140, 116, 114, 90, 213, 225, 20,
    167, 60, 69, 203, 12, 180, 198, 9, 217, 117, 38,
];

const VALID_WORKER_PUBKEY: vector<u8> = vector[
    171, 3, 39, 3, 139, 105, 166, 171, 153, 151, 102, 197, 151, 186, 140, 116, 114, 90, 213, 225, 20,
    167, 60, 69, 203, 12, 180, 198, 9, 217, 117, 38,
];

// A valid proof of possession must be generated using the same account address and protocol public key.
// If either VALID_ADDRESS or VALID_PUBKEY changed, PoP must be regenerated using [fn test_proof_of_possession].
const VALID_ADDRESS: address = @0xaf76afe6f866d8426d2be85d6ef0b11f871a251d043b2f11e15563bf418f5a5a;
// prettier-ignore
const VALID_PUBKEY: vector<u8> = x"99f25ef61f8032b914636460982c5cc6f134ef1ddae76657f2cbfec1ebfc8d097374080df6fcf0dcb8bc4b0d8e0af5d80ebbff2b4c599f54f42d6312dfc314276078c1cc347ebbbec5198be258513f386b930d02c2749a803e2330955ebd1a10";
// prettier-ignore
const PROOF_OF_POSSESSION: vector<u8> = x"b01cc86f421beca7ab4cfca87c0799c4d038c199dd399fbec1924d4d4367866dba9e84d514710b91feb65316e4ceef43";
const VALID_NET_ADDR: vector<u8> = b"/ip4/127.0.0.1/tcp/80";
const VALID_P2P_ADDR: vector<u8> = b"/ip4/127.0.0.1/udp/80";
const VALID_CONSENSUS_ADDR: vector<u8> = b"/ip4/127.0.0.1/udp/80";
const VALID_WORKER_ADDR: vector<u8> = b"/ip4/127.0.0.1/udp/80";

// Each of the presets contains the following fields:
// - Sui address
// - Protocol pubkey
// - Proof of possession
// - Network pubkey
// - Worker pubkey
// - Network address
// - P2P address
// - Consensus address
// - Worker address
const VALIDATOR_PRESET_1: vector<vector<u8>> = vector[
    x"af76afe6f866d8426d2be85d6ef0b11f871a251d043b2f11e15563bf418f5a5a",
    VALID_PUBKEY,
    PROOF_OF_POSSESSION,
    VALID_NET_PUBKEY,
    VALID_WORKER_PUBKEY,
    VALID_NET_ADDR,
    VALID_P2P_ADDR,
    VALID_CONSENSUS_ADDR,
    VALID_WORKER_ADDR,
];

/// Builder for a Validator. Contains all the fields that can be set for a
/// validator with default stabs.
public struct ValidatorBuilder has drop {
    sui_address: Option<address>,
    protocol_pubkey_bytes: Option<vector<u8>>,
    network_pubkey_bytes: Option<vector<u8>>,
    worker_pubkey_bytes: Option<vector<u8>>,
    proof_of_possession: Option<vector<u8>>,
    name: Option<vector<u8>>,
    description: Option<vector<u8>>,
    image_url: Option<vector<u8>>,
    project_url: Option<vector<u8>>,
    net_address: Option<vector<u8>>,
    p2p_address: Option<vector<u8>>,
    primary_address: Option<vector<u8>>,
    worker_address: Option<vector<u8>>,
    gas_price: Option<u64>,
    commission_rate: Option<u64>,
    is_active_at_genesis: bool,
    initial_stake: Option<u64>,
}

/// Start the builder with empty values.
public fun new(): ValidatorBuilder {
    ValidatorBuilder {
        sui_address: option::none(),
        protocol_pubkey_bytes: option::none(),
        network_pubkey_bytes: option::none(),
        worker_pubkey_bytes: option::none(),
        proof_of_possession: option::none(),
        name: option::none(),
        description: option::none(),
        image_url: option::none(),
        project_url: option::none(),
        net_address: option::none(),
        p2p_address: option::none(),
        primary_address: option::none(),
        worker_address: option::none(),
        gas_price: option::none(),
        commission_rate: option::none(),
        is_active_at_genesis: false,
        initial_stake: option::none(),
    }
}

/// Start the builder with correct default values.
public fun preset(): ValidatorBuilder {
    ValidatorBuilder {
        sui_address: option::some(VALID_ADDRESS),
        protocol_pubkey_bytes: option::some(VALID_PUBKEY),
        network_pubkey_bytes: option::some(VALID_NET_PUBKEY),
        worker_pubkey_bytes: option::some(VALID_WORKER_PUBKEY),
        proof_of_possession: option::some(PROOF_OF_POSSESSION),
        name: option::some(b"name"),
        description: option::some(b"description"),
        image_url: option::some(b"image_url"),
        project_url: option::some(b"project_url"),
        net_address: option::some(VALID_NET_ADDR),
        p2p_address: option::some(VALID_P2P_ADDR),
        primary_address: option::some(VALID_CONSENSUS_ADDR),
        worker_address: option::some(VALID_WORKER_ADDR),
        gas_price: option::none(),
        commission_rate: option::none(),
        is_active_at_genesis: false,
        initial_stake: option::none(),
    }
}

/// Build a `Validator` struct using default unchecked values.
public fun build(builder: ValidatorBuilder, ctx: &mut TxContext): Validator {
    let ValidatorBuilder {
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
        gas_price,
        commission_rate,
        is_active_at_genesis,
        initial_stake,
    } = builder;

    validator::new_for_testing(
        sui_address.destroy_or!(ctx.fresh_object_address()),
        protocol_pubkey_bytes.destroy_or!(b"protocol_pubkey_bytes"),
        network_pubkey_bytes.destroy_or!(b"network_pubkey_bytes"),
        worker_pubkey_bytes.destroy_or!(b"worker_pubkey_bytes"),
        proof_of_possession.destroy_or!(b"proof_of_possession"),
        name.destroy_or!(b"default_name"),
        description.destroy_or!(b"default_description"),
        image_url.destroy_or!(b"default_image_url"),
        project_url.destroy_or!(b"default_project_url"),
        net_address.destroy_or!(b"default_net_address"),
        p2p_address.destroy_or!(b"p2p_address"),
        primary_address.destroy_or!(b"primary_address"),
        worker_address.destroy_or!(b"worker_address"),
        initial_stake.map!(|amount| balance::create_for_testing<SUI>(amount * 1_000_000_000)),
        gas_price.destroy_or!(1),
        commission_rate.destroy_or!(0),
        is_active_at_genesis,
        ctx,
    )
}

public fun build_metadata(builder: ValidatorBuilder, ctx: &mut TxContext): ValidatorMetadata {
    let ValidatorBuilder {
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
        initial_stake,
        ..,
    } = builder;

    initial_stake.destroy_none();

    validator::new_metadata(
        sui_address.destroy_or!(VALID_ADDRESS),
        protocol_pubkey_bytes.destroy_or!(VALID_PUBKEY),
        network_pubkey_bytes.destroy_or!(VALID_NET_PUBKEY),
        worker_pubkey_bytes.destroy_or!(VALID_WORKER_PUBKEY),
        proof_of_possession.destroy_or!(PROOF_OF_POSSESSION),
        name.destroy_or!(b"name").to_string(),
        description.destroy_or!(b"description").to_string(),
        url::new_unsafe_from_bytes(image_url.destroy_or!(b"image_url")),
        url::new_unsafe_from_bytes(project_url.destroy_or!(b"project_url")),
        net_address.destroy_or!(VALID_NET_ADDR).to_string(),
        p2p_address.destroy_or!(VALID_P2P_ADDR).to_string(),
        primary_address.destroy_or!(VALID_CONSENSUS_ADDR).to_string(),
        worker_address.destroy_or!(VALID_WORKER_ADDR).to_string(),
        bag::new(ctx),
    )
}

// === Setters ===

public fun sui_address(mut builder: ValidatorBuilder, sui_address: address): ValidatorBuilder {
    builder.sui_address = option::some(sui_address);
    builder
}

public fun protocol_pubkey_bytes(
    mut builder: ValidatorBuilder,
    protocol_pubkey_bytes: vector<u8>,
): ValidatorBuilder {
    builder.protocol_pubkey_bytes = option::some(protocol_pubkey_bytes);
    builder
}

public fun network_pubkey_bytes(
    mut builder: ValidatorBuilder,
    network_pubkey_bytes: vector<u8>,
): ValidatorBuilder {
    builder.network_pubkey_bytes = option::some(network_pubkey_bytes);
    builder
}

public fun worker_pubkey_bytes(
    mut builder: ValidatorBuilder,
    worker_pubkey_bytes: vector<u8>,
): ValidatorBuilder {
    builder.worker_pubkey_bytes = option::some(worker_pubkey_bytes);
    builder
}

public fun proof_of_possession(
    mut builder: ValidatorBuilder,
    proof_of_possession: vector<u8>,
): ValidatorBuilder {
    builder.proof_of_possession = option::some(proof_of_possession);
    builder
}

public fun name(mut builder: ValidatorBuilder, name: vector<u8>): ValidatorBuilder {
    builder.name = option::some(name);
    builder
}

public fun description(mut builder: ValidatorBuilder, description: vector<u8>): ValidatorBuilder {
    builder.description = option::some(description);
    builder
}

public fun image_url(mut builder: ValidatorBuilder, image_url: vector<u8>): ValidatorBuilder {
    builder.image_url = option::some(image_url);
    builder
}

public fun project_url(mut builder: ValidatorBuilder, project_url: vector<u8>): ValidatorBuilder {
    builder.project_url = option::some(project_url);
    builder
}

public fun net_address(mut builder: ValidatorBuilder, net_address: vector<u8>): ValidatorBuilder {
    builder.net_address = option::some(net_address);
    builder
}

public fun p2p_address(mut builder: ValidatorBuilder, p2p_address: vector<u8>): ValidatorBuilder {
    builder.p2p_address = option::some(p2p_address);
    builder
}

public fun primary_address(
    mut builder: ValidatorBuilder,
    primary_address: vector<u8>,
): ValidatorBuilder {
    builder.primary_address = option::some(primary_address);
    builder
}

public fun worker_address(
    mut builder: ValidatorBuilder,
    worker_address: vector<u8>,
): ValidatorBuilder {
    builder.worker_address = option::some(worker_address);
    builder
}

public fun gas_price(mut builder: ValidatorBuilder, gas_price: u64): ValidatorBuilder {
    builder.gas_price = option::some(gas_price);
    builder
}

public fun commission_rate(mut builder: ValidatorBuilder, commission_rate: u64): ValidatorBuilder {
    builder.commission_rate = option::some(commission_rate);
    builder
}

/// Set the genesis stake for the validator.
public fun initial_stake(mut builder: ValidatorBuilder, initial_stake: u64): ValidatorBuilder {
    builder.initial_stake = option::some(initial_stake);
    builder
}

/// Try to set the genesis stake if it is not already set.
/// Used by the `TestRunner` to set the initial stake for a validator in genesis.
public fun try_initial_stake(mut builder: ValidatorBuilder, initial_stake: u64): ValidatorBuilder {
    if (builder.initial_stake.is_none()) builder.initial_stake.fill(initial_stake);
    builder
}

public fun is_active_at_genesis(
    mut builder: ValidatorBuilder,
    is_active_at_genesis: bool,
): ValidatorBuilder {
    builder.is_active_at_genesis = is_active_at_genesis;
    builder
}

// === Constants Access ===

public fun valid_protocol_pubkey(): vector<u8> { VALID_PUBKEY }

public fun valid_net_pubkey(): vector<u8> { VALID_NET_PUBKEY }

public fun valid_worker_pubkey(): vector<u8> { VALID_WORKER_PUBKEY }

public fun valid_proof_of_possession(): vector<u8> { PROOF_OF_POSSESSION }

public fun valid_net_addr(): vector<u8> { VALID_NET_ADDR }

public fun valid_p2p_addr(): vector<u8> { VALID_P2P_ADDR }

public fun valid_consensus_addr(): vector<u8> { VALID_CONSENSUS_ADDR }

public fun valid_worker_addr(): vector<u8> { VALID_WORKER_ADDR }

public fun valid_sui_address(): address { VALID_ADDRESS }

public fun valid_pubkey(): vector<u8> { VALID_PUBKEY }

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui_system::validator_builder;

use sui::balance;
use sui::sui::SUI;
use sui_system::validator::{Self, Validator};

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
        is_active_at_genesis: true,
        initial_stake: option::none(),
    }
}

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
        name.destroy_or!(b"name"),
        description.destroy_or!(b"description"),
        image_url.destroy_or!(b"image_url"),
        project_url.destroy_or!(b"project_url"),
        net_address.destroy_or!(b"net_address"),
        p2p_address.destroy_or!(b"p2p_address"),
        primary_address.destroy_or!(b"primary_address"),
        worker_address.destroy_or!(b"worker_address"),
        initial_stake.map!(|amount| balance::create_for_testing<SUI>(amount)),
        gas_price.destroy_or!(1),
        commission_rate.destroy_or!(1),
        is_active_at_genesis,
        ctx,
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

public fun initial_stake(mut builder: ValidatorBuilder, initial_stake: u64): ValidatorBuilder {
    builder.initial_stake = option::some(initial_stake);
    builder
}

public fun is_active_at_genesis(mut builder: ValidatorBuilder, is_active_at_genesis: bool): ValidatorBuilder {
    builder.is_active_at_genesis = is_active_at_genesis;
    builder
}

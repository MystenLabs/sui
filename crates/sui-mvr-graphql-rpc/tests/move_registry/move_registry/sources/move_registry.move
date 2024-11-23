// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module move_registry::move_registry;

use move_registry::name;
use std::string::String;
use sui::dynamic_field as df;
use sui::vec_map::{Self, VecMap};

public struct AppInfo has copy, store, drop {
    package_info_id: Option<ID>,
    package_address: Option<address>,
    upgrade_cap_id: Option<ID>,
}

public struct AppRecord has store {
    /// The Capability object used for managing the `AppRecord`.
    app_cap_id: ID,
    /// The SuiNS registration object that created this record.
    ns_nft_id: ID,
    // The mainnet `AppInfo` object. This is optional until a `mainnet` package
    // is mapped to a record, making the record immutable.
    app_info: Option<AppInfo>,
    // This is what being resolved for external networks.
    networks: VecMap<String, AppInfo>,
    // Any read-only metadata for the record.
    metadata: VecMap<String, String>,
    // Any extra data that needs to be stored.
    // Unblocks TTO, and DFs extendability.
    storage: UID,
}

/// The shared object holding the registry of packages.
/// There are no "admin" actions for this registry.
///
/// For simplicity, on testing, we attach all names directly to the MoveRegistry.
public struct MoveRegistry has key {
    id: UID,
}

fun init(ctx: &mut TxContext) {
    transfer::share_object(MoveRegistry {
        id: object::new(ctx),
    });
}

public fun add_record(
    registry: &mut MoveRegistry,
    name: String,
    org: String,
    package_address: ID,
    ctx: &mut TxContext,
) {
    df::add(
        &mut registry.id,
        name::new(name, org),
        AppRecord {
            app_cap_id: @0x0.to_id(),
            ns_nft_id: @0x0.to_id(),
            app_info: option::some(AppInfo {
                package_info_id: option::some(package_address),
                package_address: option::some(package_address.to_address()),
                upgrade_cap_id: option::none(),
            }),
            networks: vec_map::empty(),
            metadata: vec_map::empty(),
            storage: object::new(ctx),
        },
    );
}

/// Sets a network's value for a given app name.
public fun set_network(
    registry: &mut MoveRegistry,
    name: String,
    org: String,
    package_address: address,
    chain_id: String,
) {
    let on_chain_name = name::new(name, org);
    let record: &mut AppRecord = df::borrow_mut(
        &mut registry.id,
        on_chain_name,
    );

    if (record.networks.contains(&chain_id)) {
        record.networks.remove(&chain_id);
    };
    record
        .networks
        .insert(
            chain_id,
            AppInfo {
                package_info_id: option::some(package_address.to_id()),
                package_address: option::some(package_address),
                upgrade_cap_id: option::none(),
            },
        );
}

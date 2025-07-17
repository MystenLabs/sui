// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Defines the system object for managing coin data in a central
/// registry.

module sui::coin_registry;

use std::string::String;
use std::type_name::TypeName;
use sui::balance::Supply;
use sui::dynamic_object_field;
use sui::transfer::Receiving;
use sui::vec_map::{Self, VecMap};

// Allows calling `.add_dof(object, name, value)` on `UID`
use fun dynamic_object_field::add as UID.add_dof;
// Allows calling `.borrow_dof(object, name)` on `UID`
use fun dynamic_object_field::borrow as UID.borrow_dof;
// Allows calling `.borrow_dof_mut(object, name)` on `UID`
use fun dynamic_object_field::borrow_mut as UID.borrow_dof_mut;
// Allows calling `.exists_dof(object, name)` on `UID`
use fun dynamic_object_field::exists_ as UID.exists_dof;

/// No CoinData found for this coin type.
const ECoinDataNotFound: u64 = 0;
/// Metadata cap already claimed
const EMetadataCapAlreadyClaimed: u64 = 1;
/// Only the system address can create the registry
const ENotSystemAddress: u64 = 2;
/// CoinData for this coin type already exists
const ECoinDataAlreadyExists: u64 = 3;

const REGULATED_COIN_VARIANT: u8 = 0;

/// System object found at address 0xc that stores coin data
public struct CoinRegistry has key, store {
    id: UID,
}

/// Store only object that enables more flexible coin data
/// registration, allowing for additional fields to be added
/// without changing the CoinData structure.
#[allow(unused_field)]
public struct ExtraField(TypeName, vector<u8>) has store;

/// Key used to access coin metadata hung off the `CoinRegistry`
/// object. This key can be versioned to allow for future changes
/// to the metadata object.
public struct CoinDataKey<phantom T>() has copy, drop, store;

/// Capability object that enables coin metadata to be updated.
public struct MetadataCap<phantom T> has key, store { id: UID }

/// CoinData object that stores information about a coin type.
public struct CoinData<phantom T> has key, store {
    id: UID,
    decimals: u8,
    name: String,
    symbol: String,
    description: String,
    icon_url: String,
    supply: Option<SupplyState<T>>,
    regulated: RegulatedState,
    treasury_cap_id: Option<ID>,
    metadata_cap_id: Option<ID>,
    extra_fields: VecMap<String, ExtraField>,
}

/// Supply state of a coin type, which can be fixed or unknown.
public enum SupplyState<phantom T> has store {
    Fixed(Supply<T>),
    Unknown,
}

/// Regulated state of a coin type, which can be regulated with a deny cap,
public enum RegulatedState has copy, drop, store {
    Regulated { cap: ID, variant: u8 },
    Unknown,
}

// hot potato pattern to enforce registration after "create_currency" data creation
public struct InitCoinData<phantom T> {
    data: CoinData<T>,
}

public fun coin_registry_id(): ID {
    @0xc.to_id()
}

public fun id(registry: &CoinRegistry): ID {
    registry.id.to_inner()
}

// called after `create_currency_v2` to register the coin data
public fun transfer_to_registry<T>(init: InitCoinData<T>) {
    let InitCoinData { data } = init;

    transfer::transfer(
        data,
        coin_registry_id().to_address(),
    );
}

/// Enables CoinData to be registreed in the `CoinRegistry` object
/// via TTO.
public fun migrate_receiving<T>(registry: &mut CoinRegistry, coin_data: Receiving<CoinData<T>>) {
    let received_data = transfer::public_receive(&mut registry.id, coin_data);
    registry.register_coin_data(received_data);
}

// === CoinData Setters  ===

/// Enables a metadata cap holder to update a coin's name.
public fun set_name<T>(registry: &mut CoinRegistry, _: &MetadataCap<T>, name: String) {
    registry.data_mut<T>().name = name;
}

/// Enables a metadata cap holder to update a coin's symbol.
public fun set_symbol<T>(registry: &mut CoinRegistry, _: &MetadataCap<T>, symbol: String) {
    registry.data_mut<T>().symbol = symbol;
}

/// Enables a metadata cap holder to update a coin's description.
public fun set_description<T>(
    registry: &mut CoinRegistry,
    _: &MetadataCap<T>,
    description: String,
) {
    registry.data_mut<T>().description = description;
}

/// Enables a metadata cap holder to update a coin's icon URL.
public fun set_icon_url<T>(registry: &mut CoinRegistry, _: &MetadataCap<T>, icon_url: String) {
    registry.data_mut<T>().icon_url = icon_url;
}

public fun data<T>(registry: &CoinRegistry): &CoinData<T> {
    assert!(registry.exists<T>(), ECoinDataNotFound);
    registry.id.borrow_dof(CoinDataKey<T>())
}

// === Public getters  ===

public fun decimals<T>(coin_data: &CoinData<T>): u8 { coin_data.decimals }

public fun name<T>(coin_data: &CoinData<T>): String { coin_data.name }

public fun symbol<T>(coin_data: &CoinData<T>): String { coin_data.symbol }

public fun description<T>(coin_data: &CoinData<T>): String { coin_data.description }

public fun icon_url<T>(coin_data: &CoinData<T>): String { coin_data.icon_url }

public fun meta_data_cap_claimed<T>(coin_data: &CoinData<T>): bool {
    coin_data.metadata_cap_id.is_some()
}

public fun treasury_cap<T>(coin_data: &CoinData<T>): Option<ID> { coin_data.treasury_cap_id }

public fun deny_cap<T>(coin_data: &CoinData<T>): Option<ID> {
    match (coin_data.regulated) {
        RegulatedState::Regulated { cap, .. } => option::some(cap),
        RegulatedState::Unknown => option::none(),
    }
}

public fun supply_registered<T>(coin_data: &CoinData<T>): bool {
    match (coin_data.supply.borrow()) {
        SupplyState::Fixed(_) => true,
        SupplyState::Unknown => false,
    }
}

public fun exists<T>(registry: &CoinRegistry): bool {
    registry.id.exists_dof(CoinDataKey<T>())
}

public fun inner<T>(init: &InitCoinData<T>): &CoinData<T> {
    &init.data
}

// === Internal registration functions  ===

public(package) fun register_supply<T>(registry: &mut CoinRegistry, supply: Supply<T>) {
    assert!(registry.exists<T>(), ECoinDataNotFound);
    match (registry.data_mut<T>().supply.swap(SupplyState::Fixed(supply))) {
        SupplyState::Fixed(_supply) => abort,
        SupplyState::Unknown => (),
    };
}

public(package) fun register_regulated<T>(registry: &mut CoinRegistry, deny_cap_id: ID) {
    assert!(registry.exists<T>(), ECoinDataNotFound);
    registry.data_mut<T>().regulated =
        RegulatedState::Regulated {
            cap: deny_cap_id,
            variant: REGULATED_COIN_VARIANT,
        };
}

public(package) fun set_decimals<T>(data: &mut CoinData<T>, decimals: u8) {
    data.decimals = decimals;
}

public(package) fun set_supply<T>(data: &mut CoinData<T>, supply: Supply<T>) {
    match (data.supply.swap(SupplyState::Fixed(supply))) {
        SupplyState::Fixed(_supply) => abort,
        SupplyState::Unknown => (),
    };
}

public(package) fun set_regulated<T>(data: &mut CoinData<T>, deny_cap_id: ID) {
    data.regulated =
        RegulatedState::Regulated {
            cap: deny_cap_id,
            variant: REGULATED_COIN_VARIANT,
        };
}

public(package) fun data_mut<T>(registry: &mut CoinRegistry): &mut CoinData<T> {
    assert!(registry.exists<T>(), ECoinDataNotFound);
    registry.id.borrow_dof_mut(CoinDataKey<T>())
}

public(package) fun register_coin_data<T>(registry: &mut CoinRegistry, data: CoinData<T>) {
    assert!(!registry.exists<T>(), ECoinDataAlreadyExists);

    registry.id.add_dof(CoinDataKey<T>(), data);
}

public(package) fun inner_mut<T>(init: &mut InitCoinData<T>): &mut CoinData<T> {
    &mut init.data
}

public(package) fun create_coin_data_init<T>(
    decimals: u8,
    name: String,
    symbol: String,
    description: String,
    icon_url: String,
    supply: Option<Supply<T>>,
    treasury_cap_id: Option<ID>,
    metadata_cap_id: Option<ID>,
    deny_cap_id: Option<ID>,
    ctx: &mut TxContext,
): InitCoinData<T> {
    InitCoinData {
        data: create_coin_data(
            decimals,
            name,
            symbol,
            description,
            icon_url,
            supply,
            treasury_cap_id,
            metadata_cap_id,
            deny_cap_id,
            ctx,
        ),
    }
}

public(package) fun create_coin_data<T>(
    decimals: u8,
    name: String,
    symbol: String,
    description: String,
    icon_url: String,
    supply: Option<Supply<T>>,
    treasury_cap_id: Option<ID>,
    metadata_cap_id: Option<ID>,
    deny_cap_id: Option<ID>,
    ctx: &mut TxContext,
): CoinData<T> {
    let supply = supply
        .map!(|supply| SupplyState::Fixed(supply))
        .or!(option::some(SupplyState::Unknown));

    let regulated_state = deny_cap_id
        .map!(|cap| RegulatedState::Regulated { cap, variant: REGULATED_COIN_VARIANT })
        .destroy_or!(RegulatedState::Unknown);

    CoinData {
        id: object::new(ctx),
        decimals,
        name,
        symbol,
        description,
        icon_url,
        supply,
        regulated: regulated_state,
        treasury_cap_id,
        metadata_cap_id,
        extra_fields: vec_map::empty(),
    }
}

public(package) fun empty<T>(ctx: &mut TxContext): CoinData<T> {
    CoinData {
        id: object::new(ctx),
        decimals: 0,
        name: b"".to_string(),
        symbol: b"".to_string(),
        description: b"".to_string(),
        icon_url: b"".to_string(),
        regulated: RegulatedState::Unknown,
        supply: option::some(SupplyState::Unknown),
        treasury_cap_id: option::none(),
        metadata_cap_id: option::none(),
        extra_fields: vec_map::empty(),
    }
}

public(package) fun create_cap<T>(data: &mut CoinData<T>, ctx: &mut TxContext): MetadataCap<T> {
    assert!(!data.meta_data_cap_claimed(), EMetadataCapAlreadyClaimed);
    let id = object::new(ctx);
    let metadata_cap_id = id.to_inner();

    data.metadata_cap_id.fill(metadata_cap_id);

    MetadataCap { id }
}

#[allow(unused_function)]
/// Create and share the singleton Registry -- this function is
/// called exactly once, during the upgrade epoch.
fun create(ctx: &TxContext) {
    assert!(ctx.sender() == @0x0, ENotSystemAddress);

    transfer::share_object(CoinRegistry {
        id: object::sui_coin_registry_object_id(),
    });
}

#[test_only]
public fun create_coin_data_registry_for_testing(ctx: &mut TxContext): CoinRegistry {
    assert!(ctx.sender() == @0x0, ENotSystemAddress);

    CoinRegistry {
        id: object::new(ctx),
    }
}

#[test_only]
public fun unwrap_for_testing<T>(init: InitCoinData<T>): CoinData<T> {
    let InitCoinData { data } = init;
    data
}

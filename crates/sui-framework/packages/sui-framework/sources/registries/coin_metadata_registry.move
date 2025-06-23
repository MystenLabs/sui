// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Defines the system object for managing coin metadata in a central
/// registry.

module sui::coin_metadata_registry;

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

#[error]
const EMetadataNotFound: vector<u8> = b"No metadata found for this coin type";
#[error]
const EAlreadyClaimed: vector<u8> = b"Metadata cap already claimed";
#[error]
const ENotSystemAddress: vector<u8> = b"Only the system address can create the registry";
#[error]
const EMetadataAlreadyExists: vector<u8> = b"Metadata for this coin type already exists";

const REGULATED_COIN_VARIANT: u8 = 0;

/// System object found at address 0xc that stores coin metadata
public struct CoinMetadataRegistry has key, store {
    id: UID,
}

/// Store only object that enables more flexible metadata
/// registration, allowing for additional fields to be added
/// without changing the metadata structure.
/// This is useful for future-proofing the metadata structure
/// and allowing for additional information to be stored
/// without requiring a new version of the metadata structure.
#[allow(unused_field)]
public struct ExtraField(TypeName, vector<u8>) has store;

/// Key used to access coin metadata hung off the `CoinMetadataRegistry`
/// object. This key can be versioned to allow for future changes
/// to the metadata object.
public struct CoinMetadataKey<phantom T>() has copy, drop, store;

/// Capability object that enables coin metadata to be updated.
public struct MetadataCap<phantom T> has key, store { id: UID }

/// Metadata object that stores information about a coin type.
public struct Metadata<phantom T> has key, store {
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

/// Supply state of a coin type, which can be fixed, fixed with override,
/// or not fixed. This enables legacy coins that technically have a fixed supply
/// to be registered as fixed supply coins, while allowing new coins to have a
/// clear supply state where the registry stores the supply object itself.
public enum SupplyState<phantom T> has store {
    Fixed(Supply<T>),
    Unknown,
}

/// Regulated state of a coin type, which can be regulated with a deny cap,
public enum RegulatedState has copy, drop, store {
    Regulated { cap: ID, variant: u8 },
    Unknown,
}

// hot potato pattern to enforce registration after "create_currency" metadata creation
public struct InitMetadata<phantom T> {
    metadata: Metadata<T>,
}

public fun coin_metadata_registry_id(): ID {
    @0xc.to_id()
}

public fun id(registry: &CoinMetadataRegistry): ID {
    registry.id.to_inner()
}

// called after `create_currency_v2` to register the metadata
public fun transfer_to_registry<T>(init: InitMetadata<T>) {
    let InitMetadata { metadata } = init;

    transfer::transfer(
        metadata,
        coin_metadata_registry_id().to_address(),
    );
}

/// Enables the migration of legacy coin metadata to the new
/// `CoinMetadataRegistry` object via TTO.
public fun migrate_receiving<T>(
    registry: &mut CoinMetadataRegistry,
    metadata: Receiving<Metadata<T>>,
) {
    let received_metadata = transfer::public_receive(&mut registry.id, metadata);
    registry.register_metadata(received_metadata);
}

// === Metadata Setters  ===

/// Enables a metadata cap holder to update a coin's name.
public fun set_name<T>(registry: &mut CoinMetadataRegistry, name: String, _: &MetadataCap<T>) {
    registry.metadata_mut<T>().name = name;
}

/// Enables a metadata cap holder to update a coin's symbol.
public fun set_symbol<T>(registry: &mut CoinMetadataRegistry, symbol: String, _: &MetadataCap<T>) {
    registry.metadata_mut<T>().symbol = symbol;
}

/// Enables a metadata cap holder to update a coin's description.
public fun set_description<T>(
    registry: &mut CoinMetadataRegistry,
    description: String,
    _: &MetadataCap<T>,
) {
    registry.metadata_mut<T>().description = description;
}

/// Enables a metadata cap holder to update a coin's icon URL.
public fun set_icon_url<T>(
    registry: &mut CoinMetadataRegistry,
    icon_url: String,
    _: &MetadataCap<T>,
) {
    registry.metadata_mut<T>().icon_url = icon_url;
}

public fun metadata<T>(registry: &CoinMetadataRegistry): &Metadata<T> {
    assert!(registry.exists<T>(), EMetadataNotFound);
    registry.id.borrow_dof(CoinMetadataKey<T>())
}

// === Public getters  ===

public fun decimals<T>(metadata: &Metadata<T>): u8 { metadata.decimals }

public fun name<T>(metadata: &Metadata<T>): String { metadata.name }

public fun symbol<T>(metadata: &Metadata<T>): String { metadata.symbol }

public fun description<T>(metadata: &Metadata<T>): String { metadata.description }

public fun icon_url<T>(metadata: &Metadata<T>): String { metadata.icon_url }

public fun cap_claimed<T>(metadata: &Metadata<T>): bool { metadata.metadata_cap_id.is_some() }

public fun treasury_cap<T>(metadata: &Metadata<T>): Option<ID> {
    metadata.treasury_cap_id
}

public fun deny_cap<T>(metadata: &Metadata<T>): Option<ID> {
    match (metadata.regulated) {
        RegulatedState::Regulated { cap, variant: _ } => option::some(cap),
        RegulatedState::Unknown => option::none(),
    }
}

public fun is_fixed_supply<T>(metadata: &Metadata<T>): bool {
    match (metadata.supply.borrow()) {
        SupplyState::Fixed(_supply) => true,
        SupplyState::Unknown => false,
    }
}

public fun exists<T>(registry: &CoinMetadataRegistry): bool {
    registry.id.exists_dof(CoinMetadataKey<T>())
}

public fun to_inner_mut<T>(init: &mut InitMetadata<T>): &mut Metadata<T> {
    &mut init.metadata
}

public fun to_inner<T>(init: &InitMetadata<T>): &Metadata<T> {
    &init.metadata
}

// === Internal registration functions  ===

public(package) fun register_supply<T>(registry: &mut CoinMetadataRegistry, supply: Supply<T>) {
    assert!(registry.exists<T>(), EMetadataNotFound);
    match (registry.metadata_mut<T>().supply.swap(SupplyState::Fixed(supply))) {
        SupplyState::Fixed(_supply) => {
            abort
        },
        SupplyState::Unknown => {},
    };
}

public(package) fun register_regulated<T>(registry: &mut CoinMetadataRegistry, deny_cap_id: ID) {
    assert!(registry.exists<T>(), EMetadataNotFound);
    registry.metadata_mut<T>().regulated =
        RegulatedState::Regulated { cap: deny_cap_id, variant: REGULATED_COIN_VARIANT };
}

public(package) fun set_decimals<T>(metadata: &mut Metadata<T>, decimals: u8) {
    metadata.decimals = decimals;
}

public(package) fun set_supply<T>(metadata: &mut Metadata<T>, supply: Supply<T>) {
    match (metadata.supply.swap(SupplyState::Fixed(supply))) {
        SupplyState::Fixed(_supply) => {
            abort
        },
        SupplyState::Unknown => {},
    };
}

public(package) fun set_regulated<T>(metadata: &mut Metadata<T>, deny_cap_id: ID) {
    metadata.regulated =
        RegulatedState::Regulated { cap: deny_cap_id, variant: REGULATED_COIN_VARIANT };
}

public(package) fun metadata_mut<T>(registry: &mut CoinMetadataRegistry): &mut Metadata<T> {
    assert!(registry.exists<T>(), EMetadataNotFound);
    registry
        .id
        .borrow_dof_mut(
            CoinMetadataKey<T>(),
        )
}

public(package) fun register_metadata<T>(
    registry: &mut CoinMetadataRegistry,
    metadata: Metadata<T>,
) {
    assert!(!registry.exists<T>(), EMetadataAlreadyExists);

    registry.id.add_dof(CoinMetadataKey<T>(), metadata);
}

public(package) fun create_metadata_init<T>(
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
): InitMetadata<T> {
    InitMetadata {
        metadata: create_metadata(
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

public(package) fun create_metadata<T>(
    decimals: u8,
    name: String,
    symbol: String,
    description: String,
    icon_url: String,
    supply: Option<Supply<T>>,
    treasury_cap_id: Option<ID>,
    metadata_cap_id: Option<ID>,
    mut deny_cap_id: Option<ID>,
    ctx: &mut TxContext,
): Metadata<T> {
    let supply_state = if (supply.is_some()) {
        SupplyState::Fixed(supply.destroy_some())
    } else {
        supply.destroy_none();
        SupplyState::Unknown
    };

    let regulated_state = if (deny_cap_id.is_some()) {
        RegulatedState::Regulated { cap: deny_cap_id.extract(), variant: REGULATED_COIN_VARIANT }
    } else {
        RegulatedState::Unknown
    };

    Metadata {
        id: object::new(ctx),
        decimals,
        name,
        symbol,
        description,
        icon_url,
        supply: option::some(supply_state),
        regulated: regulated_state,
        treasury_cap_id,
        metadata_cap_id,
        extra_fields: vec_map::empty(),
    }
}

public(package) fun empty<T>(ctx: &mut TxContext): Metadata<T> {
    Metadata {
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

public(package) fun create_cap<T>(metadata: &mut Metadata<T>, ctx: &mut TxContext): MetadataCap<T> {
    assert!(!metadata.cap_claimed(), EAlreadyClaimed);
    let cap_id = object::new(ctx);

    metadata.metadata_cap_id = option::some(cap_id.to_inner());

    MetadataCap {
        id: cap_id,
    }
}

#[allow(unused_function)]
/// Create and share the singleton Registry -- this function is
/// called exactly once, during genesis.
fun create(ctx: &TxContext) {
    assert!(ctx.sender() == @0x0, ENotSystemAddress);

    let registry = CoinMetadataRegistry {
        id: object::sui_coin_metadata_registry_object_id(),
    };

    transfer::share_object(registry);
}

#[test_only]
public fun create_metadata_registry_for_testing(ctx: &mut TxContext): CoinMetadataRegistry {
    assert!(ctx.sender() == @0x0, ENotSystemAddress);

    CoinMetadataRegistry {
        id: object::new(ctx),
    }
}

#[test_only]
public fun unwrap_for_testing<T>(init: InitMetadata<T>): Metadata<T> {
    let InitMetadata { metadata } = init;
    metadata
}

module sui::coin_metadata_registry;

use std::string::String;
use sui::address;
use sui::balance::Supply;
use sui::bcs::to_bytes;
use sui::dynamic_object_field;
use sui::table::{Self, Table};
use sui::transfer::Receiving;
use sui::vec_map::{Self, VecMap};
use sui::vec_set::VecSet;

const EMetadataNotFound: u64 = 0;
const EAlreadyClaimed: u64 = 1;
const ENotSystemAddress: u64 = 2;

public struct RegistryOverrides has store {
    regulated_hashes: VecSet<address>,
    fixed_supply_hashes: VecSet<address>,
    regulated_coins: Table<address, bool>,
    fixed_supply_coins: Table<address, bool>,
    // controversial but useful for scenario where coin no longer has TCap but wants to update metadata
    legacy_metadata_cap_owners: Table<address, bool>,
}

/// @0x10.
public struct CoinMetadataRegistry has key {
    id: UID,
    overrides: RegistryOverrides,
}

public fun coin_metadata_registry_id(): ID {
    @0x10.to_id()
}

// === Coin Metadata ===
public struct CoinMetadataKey<phantom T>() has copy, drop, store;

public struct MetadataCap<phantom T> has key, store { id: UID }

public struct Metadata<phantom T> has key, store {
    id: UID,
    decimals: u8,
    name: String,
    symbol: String,
    description: String,
    icon_url: String,
    supply: Option<Supply<T>>,
    is_fixed_supply: bool,
    is_regulated: bool,
    treasury_cap_id: Option<ID>,
    metadata_cap_id: Option<ID>,
    deny_cap_id: Option<ID>,
    extra_fields: VecMap<String, String>,
}

// hot potato pattern to enforce registration after "create_currency" metadata creation
public struct InitMetadata<phantom T> {
    metadata: Metadata<T>,
}

// "unwrap" regulated hashes into the overrides table
entry fun populate_regulated_overrides(
    registry: &mut CoinMetadataRegistry,
    coins: &mut vector<address>,
) {
    // we generate the `hash` of the addresses and amounts supplied.
    let mut hash_vec = vector::empty<u8>();
    let bytes = to_bytes(coins);

    hash_vec.append(bytes);

    let hash = sui::address::from_bytes(sui::hash::blake2b256(&hash_vec));
    assert!(registry.overrides.regulated_hashes.contains(&hash));

    coins.length().do!(|_| {
        let coin = coins.pop_back();
        registry.overrides.regulated_coins.add(coin, true);
    });
}

// "unwrap" fixed supply hashes into the overrides table
entry fun populate_fixed_supply_overrides(
    registry: &mut CoinMetadataRegistry,
    coins: &mut vector<address>,
) {
    // we generate the `hash` of the addresses and amounts supplied.
    let mut hash_vec = vector::empty<u8>();
    let bytes = to_bytes(coins);

    hash_vec.append(bytes);

    let hash = sui::address::from_bytes(sui::hash::blake2b256(&hash_vec));
    assert!(registry.overrides.fixed_supply_hashes.contains(&hash));

    coins.length().do!(|_| {
        let coin = coins.pop_back();
        registry.overrides.fixed_supply_coins.add(coin, true);
    });
}

// called after create_currency_v2 to register the metadata
public fun transfer_to_registry<T>(init: InitMetadata<T>) {
    let InitMetadata { metadata } = init;

    transfer::transfer(
        metadata,
        coin_metadata_registry_id().to_address(),
    );
}

/// TTO registration
public fun migrate_receiving<T>(
    registry: &mut CoinMetadataRegistry,
    metadata: Receiving<Metadata<T>>,
) {
    let received_metadata = transfer::public_receive(&mut registry.id, metadata);
    registry.register_metadata(received_metadata);
}

/// enables coin metadata to be claimed by owners declared in the override table
public fun claim_metadata_cap_override<T>(
    registry: &mut CoinMetadataRegistry,
    ctx: &mut TxContext,
): MetadataCap<T> {
    assert!(registry.exists<T>(), EMetadataNotFound);

    assert!(registry.overrides.legacy_metadata_cap_owners.contains(ctx.sender()));

    let metadata = registry.metadata_mut<T>();

    create_cap(metadata, ctx)
}

/// === Entry Setters ===

public fun set_name<T>(_: &MetadataCap<T>, registry: &mut CoinMetadataRegistry, name: String) {
    registry.metadata_mut<T>().name = name;
}

public fun set_symbol<T>(_: &MetadataCap<T>, registry: &mut CoinMetadataRegistry, symbol: String) {
    registry.metadata_mut<T>().symbol = symbol;
}

public fun set_description<T>(
    _: &MetadataCap<T>,
    registry: &mut CoinMetadataRegistry,
    description: String,
) {
    registry.metadata_mut<T>().description = description;
}

public fun set_icon_url<T>(
    _: &MetadataCap<T>,
    registry: &mut CoinMetadataRegistry,
    icon_url: String,
) {
    registry.metadata_mut<T>().icon_url = icon_url;
}

/// === Internal Setters ===

public(package) fun register_supply<T>(registry: &mut CoinMetadataRegistry, supply: Supply<T>) {
    assert!(registry.exists<T>(), EMetadataNotFound);
    registry.metadata_mut<T>().supply.fill(supply);
}

public(package) fun register_deny_cap<T>(registry: &mut CoinMetadataRegistry, deny_cap_id: ID) {
    assert!(registry.exists<T>(), EMetadataNotFound);
    registry.metadata_mut<T>().deny_cap_id.fill(deny_cap_id);
}

public(package) fun set_decimals<T>(metadata: &mut Metadata<T>, decimals: u8) {
    metadata.decimals = decimals;
}

public(package) fun set_supply<T>(metadata: &mut Metadata<T>, supply: Supply<T>) {
    metadata.supply.fill(supply);
}

public(package) fun set_deny_cap<T>(metadata: &mut Metadata<T>, deny_cap_id: ID) {
    metadata.deny_cap_id.fill(deny_cap_id);
}

/// === Getters ===

public fun metadata<T>(registry: &CoinMetadataRegistry): &Metadata<T> {
    assert!(registry.exists<T>(), EMetadataNotFound);
    dynamic_object_field::borrow(
        &registry.id,
        CoinMetadataKey<T>(),
    )
}

public fun decimals<T>(metadata: &Metadata<T>): u8 { metadata.decimals }

public fun name<T>(metadata: &Metadata<T>): String { metadata.name }

public fun symbol<T>(metadata: &Metadata<T>): String { metadata.symbol }

public fun description<T>(metadata: &Metadata<T>): String { metadata.description }

public fun icon_url<T>(metadata: &Metadata<T>): String { metadata.icon_url }

public fun total_fixed_supply<T>(metadata: &Metadata<T>): u64 {
    assert!(metadata.supply.is_some());
    metadata.supply.borrow().supply_value()
}

public fun cap_claimed<T>(metadata: &Metadata<T>): bool { metadata.metadata_cap_id.is_some() }

public fun is_fixed_supply<T>(metadata: &Metadata<T>): bool {
    metadata.supply.is_some() || metadata.is_fixed_supply
}

public fun is_regulated<T>(metadata: &Metadata<T>): bool {
    metadata.deny_cap_id.is_some() || metadata.is_regulated
}

public fun exists<T>(registry: &CoinMetadataRegistry): bool {
    dynamic_object_field::exists_(&registry.id, CoinMetadataKey<T>())
}

public fun to_inner_mut<T>(init: &mut InitMetadata<T>): &mut Metadata<T> {
    &mut init.metadata
}

public fun to_inner<T>(init: &InitMetadata<T>): &Metadata<T> {
    &init.metadata
}

public(package) fun metadata_mut<T>(registry: &mut CoinMetadataRegistry): &mut Metadata<T> {
    assert!(registry.exists<T>(), EMetadataNotFound);
    dynamic_object_field::borrow_mut(&mut registry.id, CoinMetadataKey<T>())
}

public(package) fun register_metadata<T>(
    registry: &mut CoinMetadataRegistry,
    mut metadata: Metadata<T>,
) {
    registry.apply_overrides(&mut metadata);

    // if the metadata already exists, delete and skip registration
    if (registry.exists<T>()) {
        let Metadata { id, mut supply, .. } = metadata;

        // if newly supplied metadata has a supply, extract and register it
        if (supply.is_some()) {
            registry.register_supply(supply.extract());
        };

        supply.destroy_none();
        id.delete();
    } else {
        dynamic_object_field::add(&mut registry.id, CoinMetadataKey<T>(), metadata);
    }
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
    deny_cap_id: Option<ID>,
    ctx: &mut TxContext,
): Metadata<T> {
    Metadata {
        id: object::new(ctx),
        decimals,
        name,
        symbol,
        description,
        icon_url,
        supply,
        treasury_cap_id,
        metadata_cap_id,
        deny_cap_id,
        extra_fields: vec_map::empty(),
        is_fixed_supply: false,
        is_regulated: false,
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
        supply: option::none(),
        treasury_cap_id: option::none(),
        metadata_cap_id: option::none(),
        deny_cap_id: option::none(),
        extra_fields: vec_map::empty(),
        is_fixed_supply: false,
        is_regulated: false,
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

fun apply_overrides<T>(registry: &CoinMetadataRegistry, metadata: &mut Metadata<T>) {
    let coin_address = address::from_bytes(std::type_name::get<T>().get_address().into_bytes());
    metadata.is_fixed_supply = registry.overrides.fixed_supply_coins.contains(coin_address);
    metadata.is_regulated = registry.overrides.regulated_coins.contains(coin_address);
}

// #[allow(unused_function)]
// /// Create and share the singleton CoinMetadataRegistry
// fun create(
//     regulated_hashes: VecSet<address>,
//     fixed_supply_hashes: VecSet<address>,
//     ctx: &mut TxContext,
// ) {
//     assert!(ctx.sender() == @0x0, ENotSystemAddress);

//     let overrides = RegistryOverrides {
//         regulated_hashes: regulated_hashes,
//         fixed_supply_hashes: fixed_supply_hashes,
//         regulated_coins: table::new(ctx),
//         fixed_supply_coins: table::new(ctx),
//         legacy_metadata_cap_owners: table::new(ctx),
//     };

//     let registry = CoinMetadataRegistry {
//         id: object::sui_coin_metadata_registry_object_id(),
//         overrides: overrides,
//     };

//     transfer::share_object(registry);
// }

public fun create_metadata_registry_for_testing(
    regulated_hashes: VecSet<address>,
    fixed_supply_hashes: VecSet<address>,
    ctx: &mut TxContext,
) {
    assert!(ctx.sender() == @0x0, ENotSystemAddress);

    let overrides = RegistryOverrides {
        regulated_hashes: regulated_hashes,
        fixed_supply_hashes: fixed_supply_hashes,
        regulated_coins: table::new(ctx),
        fixed_supply_coins: table::new(ctx),
        legacy_metadata_cap_owners: table::new(ctx),
    };

    let registry = CoinMetadataRegistry {
        id: object::new(ctx),
        overrides: overrides,
    };

    transfer::share_object(registry);
}

module sui::coin_metadata_registry;

use std::string::String;
use sui::balance::Supply;
use sui::bcs::to_bytes;
use sui::dynamic_object_field;
use sui::table::{Self, Table};
use sui::transfer::Receiving;
use sui::vec_map::{Self, VecMap};
use sui::vec_set::VecSet;

use fun dynamic_object_field::add as UID.add_dof;
use fun dynamic_object_field::borrow as UID.borrow_dof;
use fun dynamic_object_field::borrow_mut as UID.borrow_dof_mut;
use fun dynamic_object_field::exists_ as UID.exists_dof;

const EMetadataNotFound: u64 = 0;
const EAlreadyClaimed: u64 = 1;
const ENotSystemAddress: u64 = 2;
const EInvalidCoinType: u64 = 3;
const EMetadataAlreadyExists: u64 = 4;

const REGULATED_COIN_VARIANT: u8 = 0;

public struct RegistryOverrides has store {
    regulated_hashes: VecSet<address>,
    fixed_supply_hashes: VecSet<address>,
    regulated_coin_types: Table<String, bool>,
    fixed_supply_coins: Table<String, bool>,
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
    supply: Option<SupplyState<T>>,
    regulated: RegulatedState,
    treasury_cap_id: Option<ID>,
    metadata_cap_id: Option<ID>,
    extra_fields: VecMap<String, String>, // new MetadataField type
}

public enum SupplyState<phantom T> has store {
    Fixed(Supply<T>),
    FixedOverride,
    NotFixed,
}

public enum RegulatedState has copy, drop, store {
    Regulated { cap: ID, variant: u8 },
    RegulatedOverride,
    NotRegulated,
}

// hot potato pattern to enforce registration after "create_currency" metadata creation
public struct InitMetadata<phantom T> {
    metadata: Metadata<T>,
}

// "unwrap" regulated hashes into the overrides table
entry fun populate_regulated_overrides(
    registry: &mut CoinMetadataRegistry,
    mut coin_types: vector<String>,
) {
    // we generate the `hash` of the addresses and amounts supplied.
    let mut hash_vec: vector<u8> = vector[];
    let bytes = to_bytes(&coin_types);

    hash_vec.append(bytes);

    let hash = sui::address::from_bytes(sui::hash::blake2b256(&hash_vec));
    assert!(registry.overrides.regulated_hashes.contains(&hash));
    registry.overrides.regulated_hashes.remove(&hash);

    coin_types.length().do!(|_| {
        let coin_type = coin_types.pop_back();
        // protect from accidental duplicates.
        if (!registry.overrides.regulated_coin_types.contains(coin_type)) {
            registry.overrides.regulated_coin_types.add(coin_type, true);
        }
    });
}

// "unwrap" fixed supply hashes into the overrides table
entry fun populate_fixed_supply_overrides(
    registry: &mut CoinMetadataRegistry,
    mut coin_types: vector<String>,
) {
    // we generate the `hash` of the addresses and amounts supplied.
    let mut hash_vec: vector<u8> = vector[];
    let bytes = to_bytes(&coin_types);

    hash_vec.append(bytes);

    let hash = sui::address::from_bytes(sui::hash::blake2b256(&hash_vec));
    assert!(registry.overrides.fixed_supply_hashes.contains(&hash));
    registry.overrides.fixed_supply_hashes.remove(&hash);

    coin_types.length().do!(|_| {
        let coin_type = coin_types.pop_back();
        // protect from accidental duplicates.
        if (!registry.overrides.fixed_supply_coins.contains(coin_type)) {
            registry.overrides.fixed_supply_coins.add(coin_type, true);
        }
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
    match (registry.metadata_mut<T>().supply.swap(SupplyState::Fixed(supply))) {
        SupplyState::Fixed(_supply) => {
            abort
        },
        SupplyState::FixedOverride | SupplyState::NotFixed => {},
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
        SupplyState::FixedOverride | SupplyState::NotFixed => {},
    };
}

public(package) fun set_regulated<T>(metadata: &mut Metadata<T>, deny_cap_id: ID) {
    metadata.regulated =
        RegulatedState::Regulated { cap: deny_cap_id, variant: REGULATED_COIN_VARIANT };
}

/// === Getters ===

public fun metadata<T>(registry: &CoinMetadataRegistry): &Metadata<T> {
    assert!(registry.exists<T>(), EMetadataNotFound);
    registry.id.borrow_dof(CoinMetadataKey<T>())
}

public fun decimals<T>(metadata: &Metadata<T>): u8 { metadata.decimals }

public fun name<T>(metadata: &Metadata<T>): String { metadata.name }

public fun symbol<T>(metadata: &Metadata<T>): String { metadata.symbol }

public fun description<T>(metadata: &Metadata<T>): String { metadata.description }

public fun icon_url<T>(metadata: &Metadata<T>): String { metadata.icon_url }

public fun cap_claimed<T>(metadata: &Metadata<T>): bool { metadata.metadata_cap_id.is_some() }

public fun is_fixed_supply<T>(metadata: &Metadata<T>): bool {
    match (metadata.supply.borrow()) {
        SupplyState::Fixed(_supply) => true,
        SupplyState::FixedOverride => true,
        SupplyState::NotFixed => false,
    }
}

public fun is_regulated<T>(metadata: &Metadata<T>): bool {
    match (metadata.regulated) {
        RegulatedState::Regulated { cap: _, variant: _ } => true,
        RegulatedState::RegulatedOverride => true,
        RegulatedState::NotRegulated => false,
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
    mut metadata: Metadata<T>,
) {
    assert!(registry.exists<T>(), EMetadataAlreadyExists);

    registry.apply_overrides(&mut metadata);

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
        SupplyState::NotFixed
    };

    let regulated_state = if (deny_cap_id.is_some()) {
        RegulatedState::Regulated { cap: deny_cap_id.extract(), variant: REGULATED_COIN_VARIANT }
    } else {
        RegulatedState::NotRegulated
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
        regulated: RegulatedState::NotRegulated,
        supply: option::some(SupplyState::NotFixed),
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

fun apply_overrides<T>(registry: &mut CoinMetadataRegistry, metadata: &mut Metadata<T>) {
    let coin_address = std::type_name::get<T>().into_string().to_string();
    if (registry.overrides.fixed_supply_coins.contains(coin_address)) {
        match (metadata.supply.swap(SupplyState::FixedOverride)) {
            SupplyState::Fixed(_supply) => {
                registry.register_supply(_supply);
            },
            SupplyState::FixedOverride | SupplyState::NotFixed => {},
        };
    };
    if (registry.overrides.regulated_coin_types.contains(coin_address)) {
        metadata.regulated = RegulatedState::RegulatedOverride;
    };
    registry.overrides.regulated_coin_types.remove(coin_address);
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
//         regulated_coin_types: table::new(ctx),
//         fixed_supply_coins: table::new(ctx),
//         legacy_metadata_cap_owners: table::new(ctx),
//     };

//     let registry = CoinMetadataRegistry {
//         id: object::sui_coin_metadata_registry_object_id(),
//         overrides: overrides,
//     };

//     transfer::share_object(registry);
// }

#[test_only]
public fun create_metadata_registry_for_testing(
    regulated_hashes: VecSet<address>,
    fixed_supply_hashes: VecSet<address>,
    ctx: &mut TxContext,
) {
    assert!(ctx.sender() == @0x0, ENotSystemAddress);

    let overrides = RegistryOverrides {
        regulated_hashes: regulated_hashes,
        fixed_supply_hashes: fixed_supply_hashes,
        regulated_coin_types: table::new(ctx),
        fixed_supply_coins: table::new(ctx),
    };

    let registry = CoinMetadataRegistry {
        id: object::new(ctx),
        overrides: overrides,
    };

    transfer::share_object(registry);
}

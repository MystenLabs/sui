module sui::coin_metadata_registry;

use std::string::String;
use sui::balance::Supply;
use sui::dynamic_field;
use sui::package::Publisher;
use sui::transfer::Receiving;

const EMetadataNotFound: u64 = 2;
const EAlreadyClaimed: u64 = 3;
const ENotSystemAddress: u64 = 4;

/// @0x10.
public struct CoinMetadataRegistry has key { id: UID }

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
    metadata_cap_id: Option<ID>,
    deny_cap_id: Option<ID>,
}

public struct InitMetadata<phantom T> {
    metadata: Metadata<T>,
}

public(package) fun to_inner_mut<T>(init: &mut InitMetadata<T>): &mut Metadata<T> {
    &mut init.metadata
}

public fun transfer<T>(init: InitMetadata<T>) {
    let InitMetadata { metadata } = init;

    transfer::public_transfer(
        metadata,
        coin_metadata_registry_id().to_address(),
    );
}

/// This is for TTO registration flow.
public fun migrate_receiving<T>(
    registry: &mut CoinMetadataRegistry,
    metadata: Receiving<Metadata<T>>,
) {
    let received_metadata = transfer::public_receive(&mut registry.id, metadata);
    registry.attach_metadata(received_metadata);
}

/// ? Similar to the above, we need to consider allowing more than 1 cap.
public fun create_cap_for_supply<T>(
    registry: &CoinMetadataRegistry,
    _supply: &mut Supply<T>,
    publisher: &Publisher,
    ctx: &mut TxContext,
): MetadataCap<T> {
    // only coin metadata objects can be registered
    assert!(publisher.from_package<T>(), ENotSystemAddress);
    let metadata = registry.metadata<T>();
    assert!(!metadata.cap_claimed(), EAlreadyClaimed);
    MetadataCap { id: object::new(ctx) }
}

public fun init_metadata_for_supply<T>(
    registry: &mut CoinMetadataRegistry,
    _: &mut Supply<T>,
    decimals: u8,
    ctx: &mut TxContext,
) {
    let mut metadata = empty<T>(ctx);
    metadata.decimals = decimals;
    registry.attach_metadata(metadata);
}

/// === Setters ===

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

public(package) fun freeze_supply<T>(registry: &mut CoinMetadataRegistry, supply: Supply<T>) {
    registry.metadata_mut<T>().supply.fill(supply);
}

/// === Getters ===

public fun metadata<T>(registry: &CoinMetadataRegistry): &Metadata<T> {
    assert!(registry.exists<T>(), EMetadataNotFound);
    dynamic_field::borrow(
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

/// Assumptions are painful here :(
public fun is_regulated<T>(metadata: &Metadata<T>): bool { metadata.deny_cap_id.is_some() }

public fun cap_claimed<T>(metadata: &Metadata<T>): bool { metadata.metadata_cap_id.is_some() }

public fun is_fixed_supply<T>(metadata: &Metadata<T>): bool { metadata.supply.is_some() }

public(package) fun metadata_mut<T>(registry: &mut CoinMetadataRegistry): &mut Metadata<T> {
    assert!(registry.exists<T>(), EMetadataNotFound);
    dynamic_field::borrow_mut(&mut registry.id, CoinMetadataKey<T>())
}

public(package) fun exists<T>(registry: &CoinMetadataRegistry): bool {
    dynamic_field::exists_(&registry.id, CoinMetadataKey<T>())
}

public(package) fun attach_metadata<T>(registry: &mut CoinMetadataRegistry, metadata: Metadata<T>) {
    dynamic_field::add(&mut registry.id, CoinMetadataKey<T>(), metadata);
}

public(package) fun overwrite_metadata<T>(
    registry: &mut CoinMetadataRegistry,
    mut metadata: Metadata<T>,
) {
    // if metadata exists, remove it
    if (dynamic_field::exists_(&registry.id, CoinMetadataKey<T>())) {
        let old_metadata: Metadata<T> = dynamic_field::remove(
            &mut registry.id,
            CoinMetadataKey<T>(),
        );
        let Metadata<T> { id, mut supply, .. } = old_metadata;
        id.delete();
        if (supply.is_some()) {
            metadata.supply.fill(supply.extract());
        };
        supply.destroy_none();
    };
    dynamic_field::add(&mut registry.id, CoinMetadataKey<T>(), metadata);
}

public(package) fun create_metadata_init<T>(
    decimals: u8,
    name: String,
    symbol: String,
    description: String,
    icon_url: String,
    supply: Option<Supply<T>>,
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
        metadata_cap_id,
        deny_cap_id,
    }
}

fun empty<T>(ctx: &mut TxContext): Metadata<T> {
    Metadata {
        id: object::new(ctx),
        decimals: 0,
        name: b"".to_string(),
        symbol: b"".to_string(),
        description: b"".to_string(),
        icon_url: b"".to_string(),
        supply: option::none(),
        metadata_cap_id: option::none(),
        deny_cap_id: option::none(),
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

// #[allow(unused_function)]
// /// Create and share the singleton CoinMetadataRegistry
// fun create(ctx: &TxContext) {
//     assert!(ctx.sender() == @0x0, ENotSystemAddress);

//     transfer::share_object(CoinMetadataRegistry {
//         id: object::sui_coin_metadata_registry_object_id(),
//     });
// }

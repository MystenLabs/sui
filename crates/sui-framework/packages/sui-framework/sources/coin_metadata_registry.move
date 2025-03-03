module sui::coin_metadata_registry;

use std::string::String;
use sui::balance::Supply;
use sui::coin::{TreasuryCap, CoinMetadata, RegulatedCoinMetadata};
use sui::dynamic_field;
use sui::transfer::Receiving;

const ENotSystemAddress: u64 = 0;
const EMetadataAlreadyExists: u64 = 1;
const EMetadataNotFound: u64 = 2;
const EAlreadyClaimed: u64 = 3;
const EMetadataCapNotClaimed: u64 = 4;
const EAlreadyRegulated: u64 = 5;

public struct Registry has key { id: UID }

// === Coin Metadata ===
public struct CoinMetadataKey<phantom T>() has copy, drop, store;

public struct CoinMetadataCap<phantom T> has key, store { id: UID }

public struct MetadataCapClaimed<phantom T>() has copy, drop, store;

public struct Metadata<phantom T> has store {
    decimals: u8,
    name: String,
    symbol: String,
    description: String,
    icon_url: String,
    is_regulated: Option<bool>,
    supply: Option<Supply<T>>,
}

public fun metadata<T>(registry: &Registry): &Metadata<T> {
    assert!(registry.exists<T>(), EMetadataNotFound);
    dynamic_field::borrow(
        &registry.id,
        CoinMetadataKey<T>(),
    )
}

public fun mark_regulated<T>(registry: &mut Registry, _: &RegulatedCoinMetadata<T>) {
    assert!(registry.exists<T>(), EMetadataNotFound);
    let metadata: &mut Metadata<T> = dynamic_field::borrow_mut(
        &mut registry.id,
        CoinMetadataKey<T>(),
    );

    assert!(metadata.is_regulated.is_none(), EAlreadyRegulated);
    metadata.is_regulated.fill(true);
}

/// Allows the owner to freeze the currency by destroying the treasury cap.
public fun freeze_supply<T>(registry: &mut Registry, mut cap: TreasuryCap<T>) {
    assert!(registry.exists<T>(), EMetadataNotFound);
    assert!(
        dynamic_field::exists_(cap.treasury_cap_uid_mut(), MetadataCapClaimed<T>()),
        EMetadataCapNotClaimed,
    );

    let metadata: &mut Metadata<T> = dynamic_field::borrow_mut(
        &mut registry.id,
        CoinMetadataKey<T>(),
    );

    metadata.supply.fill(cap.destroy_treasury_cap())
}

public fun migrate<T>(registry: &mut Registry, metadata_v1: CoinMetadata<T>) {
    registry.attach_metadata(from_v1(&metadata_v1));
    metadata_v1.destroy_metadata()
}

public fun migrate_frozen<T>(registry: &mut Registry, metadata_v1: &CoinMetadata<T>) {
    // we only allow one migration with frozen metadata, to avoid overwriting
    // in cases of "shared" metadata objects.
    assert!(!registry.exists<T>(), EMetadataAlreadyExists);
    registry.attach_metadata(from_v1(metadata_v1));
}

public fun migrate_receiving<T>(registry: &mut Registry, metadata_v1: Receiving<CoinMetadata<T>>) {
    let received_metadata = transfer::public_receive(&mut registry.id, metadata_v1);
    let metadata = from_v1(&received_metadata);

    registry.attach_metadata(metadata);
    received_metadata.destroy_metadata()
}

/// TODO: Do we want to maybe limit this to only one per `T`?
/// The only way to do this is by adding a mutable ref to the registry,
/// and saving a `CapCreated<T>` DF.
public fun create_cap<T>(cap: &mut TreasuryCap<T>, ctx: &mut TxContext): CoinMetadataCap<T> {
    assert!(
        dynamic_field::exists_(cap.treasury_cap_uid_mut(), MetadataCapClaimed<T>()),
        EAlreadyClaimed,
    );
    CoinMetadataCap {
        id: object::new(ctx),
    }
}

/// Similar to the above, we need to consider allowing more than 1 cap.
public fun create_cap_for_supply<T>(
    // Adding &mut to make sure the owner has exclusive access to the supply.
    _supply: &mut Supply<T>,
    ctx: &mut TxContext,
): CoinMetadataCap<T> {
    CoinMetadataCap { id: object::new(ctx) }
}

public fun init_metadata_for_supply<T>(registry: &mut Registry, _: &mut Supply<T>, decimals: u8) {
    let mut metadata = empty<T>();
    metadata.decimals = decimals;
    registry.attach_metadata(metadata);
}

/// === Setters ===

public fun set_name<T>(_: &CoinMetadataCap<T>, registry: &mut Registry, name: String) {
    registry.metadata_mut<T>().name = name;
}

public fun set_symbol<T>(_: &CoinMetadataCap<T>, registry: &mut Registry, symbol: String) {
    registry.metadata_mut<T>().symbol = symbol;
}

public fun set_description<T>(
    _: &CoinMetadataCap<T>,
    registry: &mut Registry,
    description: String,
) {
    registry.metadata_mut<T>().description = description;
}

public fun set_icon_url<T>(_: &CoinMetadataCap<T>, registry: &mut Registry, icon_url: String) {
    registry.metadata_mut<T>().icon_url = icon_url;
}

/// === Getters ===

public fun decimals<T>(metadata: &Metadata<T>): u8 { metadata.decimals }

public fun name<T>(metadata: &Metadata<T>): String { metadata.name }

public fun symbol<T>(metadata: &Metadata<T>): String { metadata.symbol }

public fun description<T>(metadata: &Metadata<T>): String { metadata.description }

public fun icon_url<T>(metadata: &Metadata<T>): String { metadata.icon_url }

/// Assumptions are painful here :(
public fun is_regulated<T>(metadata: &Metadata<T>): Option<bool> { metadata.is_regulated }

public fun is_fixed_supply<T>(metadata: &Metadata<T>): bool { metadata.supply.is_some() }

fun metadata_mut<T>(registry: &mut Registry): &mut Metadata<T> {
    assert!(registry.exists<T>(), EMetadataNotFound);
    dynamic_field::borrow_mut(&mut registry.id, CoinMetadataKey<T>())
}

fun exists<T>(registry: &Registry): bool {
    dynamic_field::exists_(&registry.id, CoinMetadataKey<T>())
}

fun attach_metadata<T>(registry: &mut Registry, metadata: Metadata<T>) {
    assert!(!registry.exists<T>(), EMetadataAlreadyExists);
    dynamic_field::add(&mut registry.id, CoinMetadataKey<T>(), metadata);
}

fun from_v1<T>(metadata_v1: &CoinMetadata<T>): Metadata<T> {
    let icon_url = if (metadata_v1.get_icon_url().is_some()) {
        metadata_v1.get_icon_url().destroy_some().inner_url().to_string()
    } else {
        b"".to_string()
    };

    let mut metadata = empty<T>();
    metadata.decimals = metadata_v1.get_decimals();
    metadata.name = metadata_v1.get_name();
    metadata.symbol = metadata_v1.get_symbol().to_string();
    metadata.description = metadata_v1.get_description();
    metadata.icon_url = icon_url;

    metadata
}

fun empty<T>(): Metadata<T> {
    Metadata {
        decimals: 0,
        name: b"".to_string(),
        symbol: b"".to_string(),
        description: b"".to_string(),
        icon_url: b"".to_string(),
        is_regulated: option::none(),
        supply: option::none(),
    }
}

#[allow(unused_function)]
/// Create and share the singleton Registry
fun create(ctx: &TxContext) {
    assert!(ctx.sender() == @0x0, ENotSystemAddress);

    transfer::share_object(Registry {
        id: object::sui_coin_metadata_registry_object_id(),
    });
}

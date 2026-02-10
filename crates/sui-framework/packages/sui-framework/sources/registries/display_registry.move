// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// The Display Registry provides a centralized system for managing Display objects
/// in the Sui framework. It enables type owners to define how their objects are
/// displayed in wallets and dapps by creating Display<T> objects that contain
/// key-value pairs for rendering.
///
/// # Key Features
///
/// - **Registry-based Storage**: All Display objects are derived from the DisplayRegistry
///   using the derived_object system, ensuring consistent address derivation.
/// - **Capability-based Management**: Each Display<T> is managed via a DisplayCap<T>,
///   allowing type owners to update display fields securely.
/// - **Legacy Migration**: Supports migrating from V1 Display objects to the new
///   registry-based V2 system, with both system-level and individual migration paths.
/// - **Publisher-based Authorization**: Type owners can create and claim Display objects
///   using their Publisher object as proof of ownership.
///
/// # Creating a Display
///
/// There are two ways to create a new Display:
/// - `new()` - Using an internal Permit (for framework use)
/// - `new_with_publisher()` - Using a Publisher object (for package owners)
///
/// Both methods return a Display<T> object and a DisplayCap<T> for management.
/// The Display must be shared via `share()` to make it publicly accessible.
///
/// # Managing Display Fields
///
/// - `set()` - Add or update a key-value pair
/// - `unset()` - Remove a key-value pair
/// - `clear()` - Remove all key-value pairs
///
/// All management operations require the DisplayCap<T> for authorization.
///
/// # Migration from V1
///
/// Legacy Display objects can be migrated to V2 through:
/// - `system_migration()` - Batch migration using SystemMigrationCap
/// - `migrate_v1_to_v2()` - Individual migration by the display owner
/// - `claim()` - Claim a DisplayCap for an already-migrated Display using legacy Display
/// - `claim_with_publisher()` - Claim a DisplayCap using Publisher object
module sui::display_registry;

use std::string::String;
use sui::derived_object;
use sui::display::Display as LegacyDisplay;
use sui::package::Publisher;
use sui::vec_map::{Self, VecMap};

/// TODO: Fill this in with the programmatic address responsible for
/// migrating all V1 displays into V2.
const SYSTEM_MIGRATION_ADDRESS: address = @0xf00;

#[error(code = 0)]
const ENotSystemAddress: vector<u8> = b"This is only callable from system address.";
#[error(code = 1)]
const EDisplayAlreadyExists: vector<u8> = b"Display for the supplied type already exists.";
#[error(code = 2)]
const ECapAlreadyClaimed: vector<u8> = b"Cap for this display object has already been claimed.";
#[error(code = 3)]
const ENotValidPublisher: vector<u8> = b"The publisher is not valid for the supplied type.";
#[error(code = 4)]
const EFieldDoesNotExist: vector<u8> = b"Field does not exist in the display.";
#[error(code = 5)]
const ECapNotClaimed: vector<u8> =
    b"Cap for this display object has not been claimed so you cannot delete the legacy display yet.";

/// The root registry object for all Display instances in the system.
/// All Display<T> objects are derived from this registry using the derived_object
/// system, ensuring deterministic addresses. This is a shared object that lives
/// at a well-known address (likely `0xd`).
public struct DisplayRegistry has key { id: UID }

/// A singleton capability object used for system-level migration of all V1 Display
/// objects to V2. This capability is transferred to SYSTEM_MIGRATION_ADDRESS during
/// genesis and enables batch migration of legacy Display objects. It should be
/// destroyed after migration is complete using `destroy_system_migration_cap()`.
public struct SystemMigrationCap has key { id: UID }

/// The Display object for type `T`, containing key-value pairs that define how
/// objects of this type should be rendered in wallets and dapps.
/// 
/// This is a shared object derived from the DisplayRegistry. The `cap_id` field
/// is an Option because:
/// - New displays created via `new()` or `new_with_publisher()` have `Some(cap_id)`
/// - Displays migrated via `system_migration()` have `None` until claimed
public struct Display<phantom T> has key {
    id: UID,
    /// All the (key,value) entries for a given display object.
    fields: VecMap<String, String>,
    /// The capability object ID. It's `Option` because legacy Displays will need claiming.
    cap_id: Option<ID>,
}

/// The capability object that grants permission to manage a Display<T>.
/// Required for all mutating operations on Display (set, unset, clear).
/// This is a `key + store` object, allowing it to be transferred or stored.
public struct DisplayCap<phantom T> has key, store { id: UID }

/// The key used for deriving the instance of `Display<T>` from the DisplayRegistry.
/// Each type T has a unique DisplayKey<T>, ensuring only one Display object per type
/// can be created in the registry.
public struct DisplayKey<phantom T>() has copy, drop, store;

/// Create a new Display<T> and DisplayCap<T> for type `T`.
/// 
/// This function is intended for framework use only, as it requires an
/// `internal::Permit<T>` to prove type ownership.
/// 
/// # Returns
/// - Display<T> - Must be shared via `share()` to make it publicly accessible
/// - DisplayCap<T> - Used for managing the Display fields
/// 
/// # Aborts
/// - `EDisplayAlreadyExists` if a Display<T> already exists in the registry
public fun new<T>(
    registry: &mut DisplayRegistry,
    _: internal::Permit<T>,
    ctx: &mut TxContext,
): (Display<T>, DisplayCap<T>) {
    let (display, cap) = new_display!<T>(registry, ctx);
    (display, cap)
}

/// Create a new Display<T> and DisplayCap<T> using a Publisher object as proof
/// of type ownership.
/// 
/// This is the primary way for package owners to create Display objects for their types.
/// The Publisher must be valid for type T (created from the same package).
/// 
/// # Returns
/// - Display<T> - Must be shared via `share()` to make it publicly accessible
/// - DisplayCap<T> - Used for managing the Display fields
/// 
/// # Aborts
/// - `ENotValidPublisher` if the Publisher is not from the same package as T
/// - `EDisplayAlreadyExists` if a Display<T> already exists in the registry
#[allow(unused_mut_ref)]
public fun new_with_publisher<T>(
    registry: &mut DisplayRegistry,
    publisher: &mut Publisher,
    ctx: &mut TxContext,
): (Display<T>, DisplayCap<T>) {
    assert!(publisher.from_package<T>(), ENotValidPublisher);
    let (display, cap) = new_display!<T>(registry, ctx);
    (display, cap)
}

/// Remove a key-value pair from the Display.
/// 
/// Requires the DisplayCap<T> for authorization.
/// 
/// # Aborts
/// - `EFieldDoesNotExist` if the specified key does not exist in the display
public fun unset<T>(display: &mut Display<T>, _: &DisplayCap<T>, name: String) {
    assert!(display.fields.contains(&name), EFieldDoesNotExist);
    display.fields.remove(&name);
}

/// Set a value for the specified key, replacing any existing value.
public fun set<T>(display: &mut Display<T>, _: &DisplayCap<T>, name: String, value: String) {
    if (display.fields.contains(&name)) {
        display.fields.remove(&name);
    };
    display.fields.insert(name, value);
}

/// Remove all key-value pairs from the Display, resetting it to an empty state.
/// 
/// Requires the DisplayCap<T> for authorization.
/// This is useful when you want to completely reset the display fields.
public fun clear<T>(display: &mut Display<T>, _: &DisplayCap<T>) {
    display.fields = vec_map::empty();
}

/// Share the Display<T> object, making it publicly accessible.
/// 
/// This must be called after creating a Display via `new()` or `new_with_publisher()`
/// to make the display fields readable by wallets and indexers.
public fun share<T>(display: Display<T>) {
    transfer::share_object(display)
}

/// Claim a DisplayCap<T> for a Display that was created via system_migration().
/// 
/// This function allows the holder of a legacy Display<T> to claim the capability
/// for a Display that was migrated by the system. The legacy Display is destroyed
/// as proof of ownership.
/// 
/// # Returns
/// - DisplayCap<T> - The capability for managing the Display
/// 
/// # Aborts
/// - `ECapAlreadyClaimed` if the Display already has a cap_id set
public fun claim<T: key>(
    display: &mut Display<T>,
    legacy: LegacyDisplay<T>,
    ctx: &mut TxContext,
): DisplayCap<T> {
    assert!(display.cap_id.is_none(), ECapAlreadyClaimed);
    let cap = DisplayCap<T> { id: object::new(ctx) };
    display.cap_id.fill(cap.id.to_inner());
    legacy.destroy();
    cap
}

/// Claim a DisplayCap<T> for a Display that was created via system_migration(),
/// using a Publisher as proof of ownership.
/// 
/// This is an alternative to `claim()` that doesn't require a legacy Display.
/// The Publisher must be valid for type T.
/// 
/// # Returns
/// - DisplayCap<T> - The capability for managing the Display
/// 
/// # Aborts
/// - `ECapAlreadyClaimed` if the Display already has a cap_id set
/// - `ENotValidPublisher` if the Publisher is not from the same package as T
public fun claim_with_publisher<T: key>(
    display: &mut Display<T>,
    publisher: &mut Publisher,
    ctx: &mut TxContext,
): DisplayCap<T> {
    assert!(display.cap_id.is_none(), ECapAlreadyClaimed);
    assert!(publisher.from_package<T>(), ENotValidPublisher);
    let cap = DisplayCap<T> { id: object::new(ctx) };
    display.cap_id.fill(cap.id.to_inner());
    cap
}

/// System-level migration function for creating Display objects from legacy V1 displays.
/// 
/// This function is callable only by the holder of SystemMigrationCap and is used
/// for batch migration of Display objects during the V2 launch. It creates a shared
/// Display<T> with the provided key-value pairs, but without a cap_id (set to None).
/// The cap must be claimed later via `claim()` or `claim_with_publisher()`.
/// 
/// If a Display<T> already exists, this function silently returns to avoid batching
/// issues during migration.
/// 
/// # Requires
/// - SystemMigrationCap for authorization
public fun system_migration<T: key>(
    registry: &mut DisplayRegistry,
    _: &SystemMigrationCap,
    keys: vector<String>,
    values: vector<String>,
    _ctx: &mut TxContext,
) {
    let key = DisplayKey<T>();

    // Gracefully error to avoid batching issues if someone migrates before our script.
    if (derived_object::exists(&registry.id, key)) return;

    transfer::share_object(Display<T> {
        id: derived_object::claim(&mut registry.id, key),
        fields: vec_map::from_keys_values(keys, values),
        cap_id: option::none(),
    });
}

/// Migrate a legacy Display<T> to a new registry-based Display<T>.
/// 
/// This allows individual display owners to migrate their V1 Display objects to V2.
/// The fields from the legacy Display are copied to the new Display, and the legacy
/// Display is destroyed. This creates both the Display and DisplayCap, giving the
/// caller full control.
/// 
/// # Returns
/// - Display<T> - Must be shared via `share()` to make it publicly accessible
/// - DisplayCap<T> - Used for managing the Display fields
/// 
/// # Aborts
/// - `EDisplayAlreadyExists` if a Display<T> already exists in the registry
public fun migrate_v1_to_v2<T: key>(
    registry: &mut DisplayRegistry,
    legacy: LegacyDisplay<T>,
    ctx: &mut TxContext,
): (Display<T>, DisplayCap<T>) {
    let (mut display, cap) = new_display!<T>(registry, ctx);
    display.fields = *legacy.fields();
    legacy.destroy();

    (display, cap)
}

/// Destroy the SystemMigrationCap after completing the system-level migration.
/// 
/// This should be called once all V1 Display objects have been migrated to V2
/// via `system_migration()`. After this, only individual migrations via
/// `migrate_v1_to_v2()` will be possible.
entry fun destroy_system_migration_cap(cap: SystemMigrationCap) {
    let SystemMigrationCap { id } = cap;
    id.delete();
}

/// Delete a legacy Display<T> object.
/// 
/// This can only be called after the DisplayCap has been claimed for the V2 Display,
/// ensuring the owner has migrated properly before destroying the legacy object.
/// 
/// # Aborts
/// - `ECapNotClaimed` if the Display's cap_id is None (cap not yet claimed)
public fun delete_legacy<T: key>(display: &Display<T>, legacy: LegacyDisplay<T>) {
    assert!(display.cap_id.is_some(), ECapNotClaimed);
    legacy.destroy();
}

/// Get a read-only reference to the fields of a Display<T>.
/// 
/// Returns the VecMap containing all key-value pairs for the display.
public fun fields<T>(display: &Display<T>): &VecMap<String, String> {
    &display.fields
}

/// Get the capability ID for a Display<T>, if one has been claimed.
/// 
/// Returns `Some(ID)` if a DisplayCap has been created/claimed, or `None` if the
/// Display was created via system_migration and not yet claimed.
public fun cap_id<T>(display: &Display<T>): Option<ID> {
    display.cap_id
}

/// Package-internal function to get the address that receives the SystemMigrationCap.
public(package) fun migration_cap_receiver(): address {
    SYSTEM_MIGRATION_ADDRESS
}

/// Internal macro for creating a new Display<T> and DisplayCap<T>.
/// 
/// This macro encapsulates the common logic for creating Display objects used by
/// both `new()` and `new_with_publisher()`. It ensures the Display doesn't already
/// exist, derives the Display object from the registry, and creates a DisplayCap.
macro fun new_display<$T>(
    $registry: &mut DisplayRegistry,
    $ctx: &mut TxContext,
): (Display<$T>, DisplayCap<$T>) {
    let registry = $registry;
    let ctx = $ctx;
    let key = DisplayKey<$T>();
    assert!(!derived_object::exists(&registry.id, key), EDisplayAlreadyExists);
    let cap = DisplayCap<$T> { id: object::new(ctx) };
    let display = Display<$T> {
        id: derived_object::claim(&mut registry.id, key),
        fields: vec_map::empty(),
        cap_id: option::some(cap.id.to_inner()),
    };
    (display, cap)
}

#[allow(unused_function)]
/// Create the DisplayRegistry singleton and SystemMigrationCap during genesis.
/// 
/// This function is called only from address 0x0 during end-of-epoch processing
/// to initialize the Display Registry system. It creates the shared DisplayRegistry
/// object and transfers the SystemMigrationCap to SYSTEM_MIGRATION_ADDRESS.
fun create(ctx: &mut TxContext) {
    assert!(ctx.sender() == @0x0, ENotSystemAddress);

    transfer::share_object(DisplayRegistry {
        id: object::sui_display_registry_object_id(),
    });

    transfer::transfer(
        SystemMigrationCap { id: object::new(ctx) },
        SYSTEM_MIGRATION_ADDRESS,
    );
}

#[test_only]
/// Test-only function to create the DisplayRegistry and SystemMigrationCap.
public(package) fun create_internal(ctx: &mut TxContext) {
    create(ctx);
}

module paywalrus::managed;

use std::string::String;
use sui::bag::{Self, Bag};
use sui::dynamic_object_field as dof;
use sui::vec_set::{Self, VecSet};

/// Tries to authorize an address that's already authorized
const EAlreadyAuthorized: u64 = 0;
/// Tries to deauthorize an address that's not authorized
const ENotAuthorized: u64 = 1;
/// Tries to use the shared object with an invalid version
const EInvalidVersion: u64 = 2;
/// Tries to borrow (or claim back) an object that doesn't exist
const EObjectNotFound: u64 = 3;
/// Tries to borrow (or claim back) an object without being authorized
const EUnauthorized: u64 = 4;
/// Tries to put back an object different than the one borrowed
const EInvalidReturnObject: u64 = 5;
/// Tries to borrow using an invalid custom ID
const EInvalidCustomId: u64 = 6;

const VERSION: u8 = 1;

/// The shared object that represents the Enoki shared object,
/// which holds managed NS names.
public struct EnokiObjects has key {
    id: UID,
    version: u8,
    managed_objects: Bag,
    authorized: VecSet<address>,
}

/// The Capability object that can alter the state of the shared object.
public struct EnokiObjectsCap has key, store {
    id: UID,
}

/// The key we use on our Bag to store managed objects.
public struct EnokiManagedKey has copy, store, drop {
    id: ID,
}

/// The value that holds the custom ID, used for off-chain enoki validation,
/// the owner, who can reclaim the object, and the storage key to retrieve the
/// object.
public struct EnokiManagedValue has store {
    custom_id: String,
    owner: address,
    storage: UID,
}

/// The key to retrieve an object from an `EnokiManagedValue`.
/// This is used to store the object in the storage, and keep it indexable.
public struct ObjectKey has copy, store, drop {}

/// The hot-potato promise that the object will be returned
public struct ReturnPromise {
    id: ID,
}

/// Upon init, we create the shared object that holds the managed names.
/// We also transfer the core capability to the sender.
fun init(ctx: &mut TxContext) {
    transfer::share_object(EnokiObjects {
        id: object::new(ctx),
        version: VERSION,
        managed_objects: bag::new(ctx),
        authorized: vec_set::empty(),
    });

    transfer::public_transfer(
        EnokiObjectsCap {
            id: object::new(ctx),
        },
        ctx.sender(),
    );
}

// ** User facing functions **

/// Public function to attach an object to the enoki service.
/// This gives the Enoki backend access to borrow the object (fully),
/// but only the owner can ever get the object back
/// (so transfers / burns etc are not functional from a borrower).
public fun attach_object<T: key + store>(
    enoki: &mut EnokiObjects,
    object: T,
    custom_id: String,
    ctx: &mut TxContext,
) {
    enoki.validate_version();

    let key = EnokiManagedKey {
        id: object::id(&object),
    };

    let mut value = EnokiManagedValue {
        custom_id,
        owner: ctx.sender(),
        storage: object::new(ctx),
    };

    dof::add(&mut value.storage, ObjectKey {}, object);

    enoki.managed_objects.add(key, value);
}

/// Public function allowing reclaiming of an object from the owner.
public fun reclaim_object<T: key + store>(
    enoki: &mut EnokiObjects,
    id: ID,
    ctx: &TxContext,
): T {
    enoki.validate_version();
    assert!(
        enoki.managed_objects.contains(EnokiManagedKey { id }),
        EObjectNotFound,
    );

    let EnokiManagedValue {
        custom_id: _,
        owner,
        mut storage,
    } = enoki.managed_objects.remove(EnokiManagedKey { id });

    assert!(owner == ctx.sender(), EUnauthorized);

    let object: T = dof::remove(&mut storage, ObjectKey {});

    storage.delete();

    object
}

// ** Enoki backend functions **
public fun borrow<T: key + store>(
    enoki: &mut EnokiObjects,
    id: ID,
    ctx: &TxContext,
): (T, ReturnPromise) {
    enoki.validate_version();
    let value = enoki.internal_get_value(id, ctx);

    let object: T = dof::remove(&mut value.storage, ObjectKey {});
    (object, ReturnPromise { id })
}

/// A convenient borrow function that allows calling borrow with a strict
/// custom_id. That's a convenient function to avoid off-chain validation
/// (query + check), and shouldn't be considered a "guard" in any way.
public fun borrow_with_custom_id<T: key + store>(
    enoki: &mut EnokiObjects,
    id: ID,
    custom_id: String,
    ctx: &TxContext,
): (T, ReturnPromise) {
    enoki.validate_version();
    let value = enoki.internal_get_value(id, ctx);
    assert!(value.custom_id == custom_id, EInvalidCustomId);

    let object: T = dof::remove(&mut value.storage, ObjectKey {});
    (object, ReturnPromise { id })
}

public fun put_back<T: key + store>(
    enoki: &mut EnokiObjects,
    object: T,
    promise: ReturnPromise,
    _ctx: &TxContext,
) {
    enoki.validate_version();
    let ReturnPromise { id } = promise;
    assert!(object::id(&object) == id, EInvalidReturnObject);

    let value: &mut EnokiManagedValue = enoki
        .managed_objects
        .borrow_mut(EnokiManagedKey { id });

    dof::add(&mut value.storage, ObjectKey {}, object);
}

// ** Authorization functions **

/// Authorize an address that can borrow objects from the shared object.
public fun authorize(
    enoki: &mut EnokiObjects,
    _: &EnokiObjectsCap,
    user: address,
) {
    enoki.validate_version();
    assert!(!enoki.authorized.contains(&user), EAlreadyAuthorized);
    enoki.authorized.insert(user);
}

/// Deauthorize an address that can borrow objects from the shared object.
public fun deauthorize(
    enoki: &mut EnokiObjects,
    _: &EnokiObjectsCap,
    user: address,
) {
    enoki.validate_version();
    assert!(enoki.authorized.contains(&user), ENotAuthorized);
    enoki.authorized.remove(&user);
}

/// Deauthorize self, if called.
/// This is needed to allow an address to self-deauthorize, so we can
/// react quickly in case of a security breach, without waiting for multisig
/// operations.
public fun deauthorize_self(enoki: &mut EnokiObjects, ctx: &TxContext) {
    enoki.validate_version();
    assert!(enoki.authorized.contains(&ctx.sender()), ENotAuthorized);
    enoki.authorized.remove(&ctx.sender());
}

public fun update_version(
    enoki: &mut EnokiObjects,
    _: &EnokiObjectsCap,
    version: u8,
) {
    enoki.validate_version();
    enoki.version = version;
}

// ** Private Helpers **

/// Get the value, and validate that the caller is authorized.
fun internal_get_value(
    enoki: &mut EnokiObjects,
    id: ID,
    ctx: &TxContext,
): &mut EnokiManagedValue {
    // only authorized addresses can borrow objects
    assert!(
        enoki.managed_objects.contains(EnokiManagedKey { id }),
        EObjectNotFound,
    );
    let value: &mut EnokiManagedValue = enoki
        .managed_objects
        .borrow_mut(EnokiManagedKey { id });

    assert!(
        enoki.authorized.contains(&ctx.sender()) || value.owner == ctx.sender(),
        EUnauthorized,
    );

    value
}

/// Validate version of the shared object.
fun validate_version(enoki: &EnokiObjects) {
    assert!(enoki.version == VERSION, EInvalidVersion);
}

#[test_only]
public(package) fun create_for_testing(
    version: u8,
    ctx: &mut TxContext,
): (EnokiObjects, EnokiObjectsCap) {
    let enoki = EnokiObjects {
        id: object::new(ctx),
        version,
        managed_objects: bag::new(ctx),
        authorized: vec_set::empty(),
    };

    let cap = EnokiObjectsCap {
        id: object::new(ctx),
    };

    (enoki, cap)
}

#[test_only]
public(package) fun init_for_testing(ctx: &mut TxContext) {
    init(ctx);
}

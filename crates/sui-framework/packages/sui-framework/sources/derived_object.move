module sui::derived_object;

use sui::dynamic_field as df;

/// Tries to create an object twice
const EObjectAlreadyExists: u64 = 0;
/// The parent supplied in deletion is not valid.
const EInvalidParent: u64 = 1;

/// Added as a DF to the parent's UID, to mark an ID as claimed.
public struct Claimed(ID) has copy, drop, store;

/// An internal key to protect from generating the same UID twice (e..g collide with DFs)
public struct DerivedObjectKey<K: copy + drop + store>(K) has copy, drop, store;

/// Claim a derived UID, using the parent's UID & any key
public fun claim<K: copy + drop + store>(parent: &mut UID, key: K): UID {
    let id = derive_id(parent.to_inner(), key);

    assert!(!df::exists_(parent, Claimed(id)), EObjectAlreadyExists);

    let uid = object::new_uid_from_hash(id.to_address());

    df::add(parent, Claimed(id), true);

    uid
}

/// Allows deleting a UID that has been derived by a parent, making the
/// UID available again for claim.
public fun delete(parent: &mut UID, uid: UID) {
    let claimed = Claimed(uid.to_inner());
    assert!(df::exists_(parent, claimed), EInvalidParent);

    df::remove<_, bool>(parent, claimed);
    uid.delete();
}

public fun exists<K: copy + drop + store>(parent: &UID, key: K): bool {
    let id = derive_id(parent.to_inner(), key);
    df::exists_(parent, Claimed(id))
}

public fun derive_id<K: copy + drop + store>(parent: ID, key: K): ID {
    let hash = df::hash_type_and_key(parent.to_address(), DerivedObjectKey(key));
    hash.to_id()
}

public fun derive_address<K: copy + drop + store>(parent: ID, key: K): address {
    derive_id(parent, key).to_address()
}

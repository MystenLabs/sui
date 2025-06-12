module sui::account;

use sui::dynamic_field as df;

/// Tries to create an object twice
const EObjectAlreadyExists: u64 = 0;
/// The parent supplied in deletion is not valid.
const EInvalidParent: u64 = 1;

use fun df::add as UID.df_add;
use fun df::exists_ as UID.df_exists_;
use fun df::remove as UID.df_remove;

public struct Account has key, store {
    /// Deterministic, based on by the hash of the parent ID, the field name value and it's type,
    /// i.e. hash(parent.id || name || Name)
    id: UID,
}

/// Added as a DF to the parent, to mark a "UID" as taken.
public struct Claimed<K: copy + drop + store>(K) has copy, drop, store;

/// Internal `key` to help us make sure we cannot have collissions when generating UIDs.
public struct AccountKey<K: copy + drop + store>(K) has copy, drop, store;

/// Claim the `Account` using the parent's UID & the key
public fun claim<K: copy + drop + store>(parent: &mut UID, key: K): Account {
    let id = derive_id(parent.to_inner(), key);

    // Prevent duplicate creation of the same `Account`, based on `key`.
    assert!(!parent.df_exists_(Claimed(key)), EObjectAlreadyExists);

    let id = object::new_uid_from_hash(id.to_address());

    parent.df_add(Claimed(key), true);

    Account {
        id,
    }
}

/// Delete `Account<V>`, by presenting its parent.
public fun delete<K: copy + drop + store>(account: Account, key: K, parent: &mut UID) {
    let Account { id } = account;
    let field_id = derive_id(parent.to_inner(), key);

    assert!(id.as_inner() == field_id, EInvalidParent);

    parent.df_remove<_, bool>(Claimed(key));
    id.delete();
}

public fun exists<K: copy + drop + store>(parent: &UID, key: K): bool {
    parent.df_exists_(Claimed(key))
}

public fun derive_id<K: copy + drop + store>(parent: ID, key: K): ID {
    let hash = df::hash_type_and_key(parent.to_address(), AccountKey(key));
    hash.to_id()
}

public fun uid(account: &Account): &UID {
    &account.id
}

public fun uid_mut(account: &mut Account): &mut UID {
    &mut account.id
}



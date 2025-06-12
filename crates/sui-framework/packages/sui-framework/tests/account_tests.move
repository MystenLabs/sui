module sui::account_tests;

use sui::account;
use sui::test_utils::destroy;
use sui::dynamic_field as df;

use fun df::add as UID.df_add;
use fun df::field_id as UID.df_field_id;

use fun object::new as TxContext.new;

public struct Registry has key {
    id: UID,
}

#[test]
fun test_create_predefined_account() {
    let mut ctx = tx_context::dummy();

    let mut registry = Registry { id: ctx.new() };
    // let mut another_registry = Registry { id: ctx.new() };


    let key = b"demo".to_string();
    let another_key = b"another_demo".to_string();

    let derived_id = account::derive_id(registry.id.to_inner(), key);
    let another_derived_id = account::derive_id(registry.id.to_inner(), another_key);

    let account = account::claim(&mut registry.id, key);
    // now create a DF for the same account, and make sure the same KEY is a different object Id.

    // Add the same key as a Dynamic Field now.
    registry.id.df_add(key, true);

    assert!(registry.id.df_field_id<_, bool>(key) != account.uid().to_inner());
    
    let another_account = account::claim(&mut registry.id, another_key);

    assert!(account::exists(&registry.id, key));
    assert!(account::exists(&registry.id, another_key));
    assert!(object::id(&account) == derived_id);
    assert!(object::id(&another_account) == another_derived_id);
    assert!(account.uid().to_inner() == derived_id);
    assert!(another_account.uid().to_inner() == another_derived_id);

    account.delete(key, &mut registry.id);
    another_account.delete(another_key, &mut registry.id);

    assert!(!account::exists(&registry.id, key));
    assert!(!account::exists(&registry.id, another_key));

    destroy(registry);
}

#[test, expected_failure(abort_code = account::EObjectAlreadyExists)]
fun try_to_claim_same_account_twice() {
    let mut ctx = tx_context::dummy();
    let uid = object::new(&mut ctx);

    let mut registry = Registry { id: uid };
    let key = b"demo".to_string();

    let account = account::claim(&mut registry.id, key);
    let another_account = account::claim(&mut registry.id, key);

    abort
}

#[test, expected_failure(abort_code = account::EInvalidParent)]
fun try_to_delete_account_with_invalid_parent() {
    let mut ctx = tx_context::dummy();
    let uid = object::new(&mut ctx);
    let mut another_uid = object::new(&mut ctx);

    let mut registry = Registry { id: uid };
    let key = b"demo".to_string();

    let account = account::claim(&mut registry.id, key);

    account.delete(key, &mut another_uid);

    abort
}

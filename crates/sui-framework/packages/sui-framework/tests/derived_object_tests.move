module sui::derived_object_tests;

use sui::derived_object;
use sui::test_utils::destroy;

use fun object::new as TxContext.new;

public struct Registry has key { id: UID }

#[test]
fun test_create_predefined_account() {
    let mut ctx = tx_context::dummy();
    let mut registry = Registry { id: ctx.new() };

    let key = b"demo".to_string();
    let another_key = b"another_demo".to_string();

    let derived_id = derived_object::derive_address(registry.id.to_inner(), key);
    let another_derived_id = derived_object::derive_address(registry.id.to_inner(), another_key);

    let derived_uid = derived_object::claim(&mut registry.id, key);
    let another_derived_uid = derived_object::claim(&mut registry.id, another_key);

    assert!(derived_object::exists(&registry.id, key));
    assert!(derived_object::exists(&registry.id, another_key));

    assert!(derived_uid.to_address() == derived_id);
    assert!(another_derived_uid.to_address() == another_derived_id);

    derived_object::delete(&mut registry.id, derived_uid);
    derived_object::delete(&mut registry.id, another_derived_uid);

    assert!(!derived_object::exists(&registry.id, key));
    assert!(!derived_object::exists(&registry.id, another_key));

    destroy(registry);
}

#[test, expected_failure(abort_code = derived_object::EObjectAlreadyExists)]
fun try_to_claim_same_account_twice() {
    let mut ctx = tx_context::dummy();

    let mut registry = Registry { id: object::new(&mut ctx) };
    let key = b"demo".to_string();

    let _uid = derived_object::claim(&mut registry.id, key);
    let _another_uid = derived_object::claim(&mut registry.id, key);

    abort
}

#[test, expected_failure(abort_code = derived_object::EInvalidParent)]
fun try_to_delete_account_with_invalid_parent() {
    let mut ctx = tx_context::dummy();

    let mut registry = Registry { id: object::new(&mut ctx) };
    let mut another_registry = Registry { id: object::new(&mut ctx) };
    let key = b"demo".to_string();

    let uid = derived_object::claim(&mut registry.id, key);

    derived_object::delete(&mut another_registry.id, uid);

    abort
}

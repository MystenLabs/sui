module sui::derived_object_tests;

use sui::derived_object;
use sui::test_utils::destroy;

use fun object::new as TxContext.new;

public struct Registry has key { id: UID }

#[test]
fun create_derived_id() {
    let mut ctx = tx_context::dummy();
    let mut registry = Registry { id: ctx.new() };

    let key = b"demo".to_string();
    let another_key = b"another_demo".to_string();

    let derived_id = derived_object::derive_address(registry.id.to_inner(), key);
    let another_derived_id = derived_object::derive_address(registry.id.to_inner(), another_key);

    let derived_uid = registry.id.derive_object(key);
    let another_derived_uid = registry.id.derive_object(another_key);

    assert!(derived_object::exists(&registry.id, key));
    assert!(derived_object::exists(&registry.id, another_key));

    assert!(derived_uid.to_address() == derived_id);
    assert!(another_derived_uid.to_address() == another_derived_id);

    destroy(registry);
    destroy(derived_uid);
    destroy(another_derived_uid);
}

#[test]
fun multiple_registries_uniqueness() {
    let mut ctx = tx_context::dummy();
    let mut registry = Registry { id: ctx.new() };
    let mut another_registry = Registry { id: ctx.new() };

    let key = b"demo".to_string();

    let derived_uid = registry.id.derive_object(key);
    let another_derived_uid = another_registry.id.derive_object(key);

    assert!(derived_uid.to_address() != another_derived_uid.to_address());

    destroy(registry);
    destroy(another_registry);
    destroy(derived_uid);
    destroy(another_derived_uid);
}

#[test, expected_failure(abort_code = derived_object::EObjectAlreadyExists)]
fun try_to_claim_id_twice() {
    let mut ctx = tx_context::dummy();

    let mut registry = Registry { id: object::new(&mut ctx) };
    let key = b"demo".to_string();

    let _uid = derived_object::new(&mut registry.id, key);
    let _another_uid = derived_object::new(&mut registry.id, key);

    abort
}

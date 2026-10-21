// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::derived_object_tests;

use std::unit_test::{assert_eq, destroy};
use sui::derived_object;
use sui::test_scenario;

use fun object::new as TxContext.new;

public struct Registry has key { id: UID }

public struct Derived has key { id: UID }

public struct DemoStruct has copy, drop, store {
    value: u64,
}

public struct GenericStruct<T: copy + store + drop> has copy, drop, store {
    value: T,
}

#[test]
fun create_derived_id() {
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

    destroy(registry);
    derived_uid.delete();
    another_derived_uid.delete();
}

#[test]
fun multiple_registries_uniqueness() {
    let mut ctx = tx_context::dummy();
    let mut registry = Registry { id: ctx.new() };
    let mut another_registry = Registry { id: ctx.new() };

    let key = b"demo".to_string();

    let derived_uid = derived_object::claim(&mut registry.id, key);
    let another_derived_uid = derived_object::claim(&mut another_registry.id, key);

    assert!(derived_uid.to_address() != another_derived_uid.to_address());

    destroy(registry);
    destroy(another_registry);
    derived_uid.delete();
    another_derived_uid.delete();
}

#[test]
fun test_marker_exists_even_after_deletion() {
    let mut ctx = tx_context::dummy();
    let mut registry = Registry { id: ctx.new() };

    let key = b"persist_test".to_string();
    let derived_uid = derived_object::claim(&mut registry.id, key);

    assert!(derived_object::exists(&registry.id, key));

    derived_uid.delete();

    assert!(derived_object::exists(&registry.id, key));

    destroy(registry);
}

#[test]
fun test_derive_address_deterministic() {
    let mut ctx = tx_context::dummy();
    let registry = Registry { id: ctx.new() };

    let key = b"is deterministic".to_string();

    let addr1 = derived_object::derive_address(registry.id.to_inner(), key);
    let addr2 = derived_object::derive_address(registry.id.to_inner(), key);

    assert!(addr1 == addr2);

    destroy(registry);
}

#[test]
fun test_derive_address_deterministic_snapshot() {
    // Making sure that our derivation algorithm never changes!
    let addr1 = derived_object::derive_address(@0x2.to_id(), b"foo");
    assert_eq!(addr1, @0xa2b411aa9588c398d8e3bc97dddbdd430b5ded7f81545d05e33916c3ca0f30c3);
}

#[test]
fun test_derive_address_with_struct_key_snapshot() {
    let key = DemoStruct { value: 1 };
    let addr = derived_object::derive_address(@0x2.to_id(), key);
    assert_eq!(addr, @0x20c58d8790a5d2214c159c23f18a5fdc347211e511186353e785ad543abcea6b);
}

#[test]
fun test_derive_address_with_generic_struct_key_snapshot() {
    let key = GenericStruct<u64> { value: 1 };
    let addr = derived_object::derive_address(@0x2.to_id(), key);
    assert_eq!(addr, @0xb497b8dcf1e297ae5fa69c040e4a08ef8240d5373bbc9d6b686ffbd7dfe04cbe);
}

#[test]
fun test_similar_keys_different_addresses() {
    let mut ctx = tx_context::dummy();
    let registry = Registry { id: ctx.new() };

    let key1 = b"foo".to_string();
    let key2 = b"foo";
    let key3 = b"foo".to_ascii_string();

    let addr1 = derived_object::derive_address(registry.id.to_inner(), key1);
    let addr2 = derived_object::derive_address(registry.id.to_inner(), key2);
    let addr3 = derived_object::derive_address(registry.id.to_inner(), key3);

    assert!(addr1 != addr2);
    assert!(addr1 != addr3);
    assert!(addr2 != addr3);

    destroy(registry);
}

#[test]
fun test_similar_keys_different_addresses_2() {
    let mut ctx = tx_context::dummy();
    let registry = Registry { id: ctx.new() };

    let key1 = vector<u64>[];
    let key2 = vector<u8>[];

    let addr1 = derived_object::derive_address(registry.id.to_inner(), key1);
    let addr2 = derived_object::derive_address(registry.id.to_inner(), key2);

    assert!(addr1 != addr2);

    destroy(registry);
}

#[test, expected_failure(abort_code = derived_object::EObjectAlreadyExists)]
fun try_to_claim_id_twice() {
    let mut ctx = tx_context::dummy();

    let mut registry = Registry { id: object::new(&mut ctx) };
    let key = b"demo".to_string();

    let _uid = derived_object::claim(&mut registry.id, key);
    let _another_uid = derived_object::claim(&mut registry.id, key);

    abort
}

#[test]
fun release_and_claim_again() {
    let mut ctx = tx_context::dummy();
    let mut registry = Registry { id: ctx.new() };
    let key = b"demo".to_string();

    let uid = derived_object::claim(&mut registry.id, key);
    let addr = uid.to_address();
    derived_object::release(&mut registry.id, uid);
    assert!(!derived_object::exists(&registry.id, key));

    let uid = derived_object::claim(&mut registry.id, key);
    assert_eq!(uid.to_address(), addr);
    assert!(derived_object::exists(&registry.id, key));

    derived_object::release(&mut registry.id, uid);
    let uid = derived_object::claim(&mut registry.id, key);
    assert_eq!(uid.to_address(), addr);

    destroy(registry);
    uid.delete();
}

#[test, expected_failure(abort_code = derived_object::EObjectNotClaimed)]
fun release_with_wrong_parent() {
    let mut ctx = tx_context::dummy();
    let mut registry = Registry { id: ctx.new() };
    let mut other = Registry { id: ctx.new() };
    let uid = derived_object::claim(&mut registry.id, b"demo".to_string());
    derived_object::release(&mut other.id, uid);
    abort
}

#[test, expected_failure(abort_code = derived_object::EObjectNotClaimed)]
fun release_non_derived_uid() {
    let mut ctx = tx_context::dummy();
    let mut registry = Registry { id: ctx.new() };
    derived_object::release(&mut registry.id, ctx.new());
    abort
}

#[test, expected_failure(abort_code = derived_object::EObjectNotClaimed)]
fun release_twice() {
    let mut ctx = tx_context::dummy();
    let mut registry = Registry { id: ctx.new() };
    let key = b"demo".to_string();
    let uid = derived_object::claim(&mut registry.id, key);
    let addr = uid.to_address();
    derived_object::release(&mut registry.id, uid);
    // Unreachable outside the framework: only the package-private `new_uid_from_hash` can forge a
    // second UID for a released address.
    let forged = object::new_uid_from_hash(@0x0, addr);
    assert_eq!(forged.to_address(), addr);
    derived_object::release(&mut registry.id, forged);
    abort
}

#[test, expected_failure(abort_code = derived_object::ESharedObjectRelease)]
fun release_shared_object() {
    let sender = @0xA;
    let mut scenario = test_scenario::begin(sender);
    let mut registry = Registry { id: object::new(scenario.ctx()) };
    transfer::share_object(Derived { id: derived_object::claim(&mut registry.id, 0u64) });
    transfer::transfer(registry, sender);

    scenario.next_tx(sender);
    let mut registry = scenario.take_from_sender<Registry>();
    let Derived { id } = scenario.take_shared<Derived>();
    derived_object::release(&mut registry.id, id);
    abort
}

#[test]
fun release_owned_object_and_claim_again() {
    let sender = @0xA;
    let mut scenario = test_scenario::begin(sender);
    let mut registry = Registry { id: object::new(scenario.ctx()) };
    let derived = Derived { id: derived_object::claim(&mut registry.id, 0u64) };
    let id = object::id(&derived);
    transfer::transfer(derived, sender);
    transfer::transfer(registry, sender);

    scenario.next_tx(sender);
    let mut registry = scenario.take_from_sender<Registry>();
    let Derived { id: uid } = scenario.take_from_sender<Derived>();
    derived_object::release(&mut registry.id, uid);
    scenario.return_to_sender(registry);

    let effects = scenario.next_tx(sender);
    assert!(effects.deleted().contains(&id));
    let mut registry = scenario.take_from_sender<Registry>();
    assert!(!derived_object::exists(&registry.id, 0u64));
    transfer::transfer(Derived { id: derived_object::claim(&mut registry.id, 0u64) }, sender);
    scenario.return_to_sender(registry);

    let effects = scenario.next_tx(sender);
    assert!(effects.created().contains(&id));
    scenario.end();
}

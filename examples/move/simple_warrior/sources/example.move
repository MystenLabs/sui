// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Demonstrates wrapping objects using the `Option` type.
module simple_warrior::example;

public struct Sword has key, store {
    id: UID,
    strength: u8,
}

public struct Warrior has key, store {
    id: UID,
    sword: Option<Sword>,
}

/// Warrior already has a Sword equipped.
const EAlreadyEquipped: u64 = 0;

/// Warrior does not have a sword equipped.
const ENotEquipped: u64 = 1;

public fun new_sword(strength: u8, ctx: &mut TxContext): Sword {
    Sword { id: object::new(ctx), strength }
}

public fun new_warrior(ctx: &mut TxContext): Warrior {
    Warrior { id: object::new(ctx), sword: option::none() }
}

public fun equip(warrior: &mut Warrior, sword: Sword) {
    assert!(option::is_none(&warrior.sword), EAlreadyEquipped);
    option::fill(&mut warrior.sword, sword);
}

public fun unequip(warrior: &mut Warrior): Sword {
    assert!(option::is_some(&warrior.sword), ENotEquipped);
    option::extract(&mut warrior.sword)
}

// === Tests ===
#[test_only]
use sui::test_scenario as ts;

#[test]
fun test_equip_empty() {
    let mut ts = ts::begin(@0xA);
    let s = new_sword(42, ts::ctx(&mut ts));
    let mut w = new_warrior(ts::ctx(&mut ts));

    equip(&mut w, s);

    let Warrior { id, sword } = w;
    object::delete(id);

    let Sword { id, strength: _ } = option::destroy_some(sword);
    object::delete(id);

    ts::end(ts);
}

#[test]
fun test_equip_unequip() {
    let mut ts = ts::begin(@0xA);
    let s1 = new_sword(21, ts::ctx(&mut ts));
    let s2 = new_sword(42, ts::ctx(&mut ts));
    let mut w = new_warrior(ts::ctx(&mut ts));

    equip(&mut w, s1);

    let Sword { id, strength } = unequip(&mut w);
    assert!(strength == 21, 0);
    object::delete(id);

    equip(&mut w, s2);

    let Warrior { id, sword } = w;
    object::delete(id);

    let Sword { id, strength } = option::destroy_some(sword);
    assert!(strength == 42, 0);
    object::delete(id);

    ts::end(ts);
}

#[test]
#[expected_failure(abort_code = ENotEquipped)]
fun test_unequip_empty() {
    let mut ts = ts::begin(@0xA);
    let mut w = new_warrior(ts::ctx(&mut ts));
    let _s = unequip(&mut w);
    abort 1337
}

#[test]
#[expected_failure(abort_code = EAlreadyEquipped)]
fun test_equip_already_equipped() {
    let mut ts = ts::begin(@0xA);
    let s1 = new_sword(21, ts::ctx(&mut ts));
    let s2 = new_sword(42, ts::ctx(&mut ts));
    let mut w = new_warrior(ts::ctx(&mut ts));

    equip(&mut w, s1);
    equip(&mut w, s2);

    abort 1337
}

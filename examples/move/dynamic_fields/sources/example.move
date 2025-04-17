// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module dynamic_fields::example;

use sui::dynamic_object_field as ofield;

public struct Parent has key {
    id: UID,
}

public struct Child has key, store {
    id: UID,
    count: u64,
}

public fun add_child(parent: &mut Parent, child: Child) {
    ofield::add(&mut parent.id, b"child", child);
}

/// If `child` is a dynamic field of some `Parent`, then this
/// function cannot be called directly, because `child` must be
/// accessed via its parent.
///
/// Use this function as a transaction entry-point if `child` is
/// address-owned or shared, and use `mutate_child_via_parent` if
/// it is a dynamic field of a `Parent`.
///
/// This restriction only applies on transaction entry.  Within
/// Move, if you have borrowed a `Child` that is a dynamic field
/// of a `Parent`, it is possible to call `mutate_child` on it.
public fun mutate_child(child: &mut Child) {
    child.count = child.count + 1;
}

public fun mutate_child_via_parent(parent: &mut Parent) {
    mutate_child(ofield::borrow_mut(&mut parent.id, b"child"))
}

public fun reclaim_child(parent: &mut Parent): Child {
    ofield::remove(&mut parent.id, b"child")
}

public fun delete_child(parent: &mut Parent) {
    let Child { id, count: _ } = reclaim_child(parent);
    object::delete(id);
}

// === Tests ===
#[test_only]
use sui::test_scenario;

#[test]
fun test_add_delete() {
    let mut ts = test_scenario::begin(@0xA);
    let ctx = ts.ctx();

    let mut p = Parent { id: object::new(ctx) };
    p.add_child(Child { id: object::new(ctx), count: 0 });

    p.mutate_child_via_parent();
    p.delete_child();

    let Parent { id } = p;
    id.delete();

    ts.end();
}

#[test]
fun test_add_reclaim() {
    let mut ts = test_scenario::begin(@0xA);
    let ctx = ts.ctx();

    let mut p = Parent { id: object::new(ctx) };
    p.add_child(Child { id: object::new(ctx), count: 0 });

    p.mutate_child_via_parent();

    let mut c = p.reclaim_child();
    assert!(c.count == 1, 0);

    c.mutate_child();
    assert!(c.count == 2, 1);

    let Child { id, count: _ } = c;
    id.delete();

    let Parent { id } = p;
    id.delete();

    ts.end();
}

#[test]
/// This is not a desirable property, but objects can be deleted
/// with dynamic fields still attached, and they become
/// inaccessible.
fun test_delete_with_child_attached() {
    let mut ts = test_scenario::begin(@0xA);
    let ctx = ts.ctx();

    let mut p = Parent { id: object::new(ctx) };
    p.add_child(Child { id: object::new(ctx), count: 0 });

    let Parent { id } = p;
    id.delete();

    ts.end();
}

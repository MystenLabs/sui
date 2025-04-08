// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Implements a soul-bound primitive for objects. Unlike a typical Soulbound
/// concept used with accounts, the object bound can not be directly referenced
/// or mutated by its object owner.
///
/// To bypass the limitation with object, the "Transfer To Object" feature is
/// used to receive and then send the object back to its object-owner.
module obo::object_bound;

use sui::transfer::Receiving;

/// Trying to return a different object than the one that was borrowed.
const EDontMessWithMe: u64 = 0;

/// An object bound to a specific object. Created with the intention of
/// being returned to the original owner.
public struct ObjectBound<T: key + store> has key {
    id: UID,
    `for`: address,
    inner: Option<T>,
}

/// A HotPotato ensuring that the object is returned and sent back to the
/// original owner.
public struct Borrow<T: key + store> { object: ObjectBound<T>, inner_id: ID }

/// Create and send an ObjectBound.
public fun new<T: key + store>(inner: T, `for`: address, ctx: &mut TxContext) {
    transfer::transfer(
        ObjectBound {
            `for`,
            id: object::new(ctx),
            inner: option::some(inner),
        },
        `for`,
    );
}

/// Receive and use an ObjectBound.
public fun borrow<T: key + store>(
    parent: &mut UID,
    to_receive: Receiving<ObjectBound<T>>,
): (T, Borrow<T>) {
    let mut object = transfer::receive(parent, to_receive);
    let inner = option::extract(&mut object.inner);
    let inner_id = object::id(&inner);

    (inner, Borrow { object, inner_id })
}

/// Store an ObjectBound.
public fun store<T: key + store>(inner: T, borrow: Borrow<T>) {
    assert!(object::id(&inner) == borrow.inner_id, EDontMessWithMe);
    let Borrow { mut object, inner_id: _ } = borrow;
    let `for` = object.`for`;
    option::fill(&mut object.inner, inner);
    transfer::transfer(object, `for`);
}

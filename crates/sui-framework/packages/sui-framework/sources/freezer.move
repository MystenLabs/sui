// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// This module safely freezes an object to prevent it from ever being mutated or transferred.
module sui::freezer;

use sui::dynamic_object_field as dof;

/// Dynamic Object Field key to store an object.
public struct Key() has store, copy, drop;

/// We store an object inside `Ice` to prevent it from being accessible. 
public struct Ice<phantom T> has key {
    id: UID,
}

/// It saves `data` as a dynamic field on `Ice` instead of wrapping it for discoverability.
public fun freeze_object<T: key + store>(data: T, ctx: &mut TxContext) {
    let mut ice = Ice<T> {
        id: object::new(ctx),
    };

    dof::add(&mut ice.id, Key(), data);

    transfer::freeze_object(ice);
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Exercises shared object deletion (`shared_object_deletion`). The surfer can
/// pass the shared object by value to `delete`, which consensus must order and
/// the execution layer must allow.
module move_building_blocks::shared_deletion {
    public struct Deletable has key, store {
        id: UID,
        value: u64,
    }

    public fun create(value: u64, ctx: &mut TxContext) {
        transfer::share_object(Deletable { id: object::new(ctx), value });
    }

    public fun mutate(obj: &mut Deletable, value: u64) {
        obj.value = value;
    }

    public fun delete(obj: Deletable) {
        let Deletable { id, value: _ } = obj;
        object::delete(id);
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Exercises dynamic fields and dynamic object fields directly (the existing
/// `objects`/`limits` modules only touch them indirectly via `Table`).
module move_building_blocks::dynamic_fields {
    use sui::dynamic_field;
    use sui::dynamic_object_field;

    public struct Holder has key, store {
        id: UID,
    }

    public struct Child has key, store {
        id: UID,
        value: u64,
    }

    public fun create_holder(ctx: &mut TxContext) {
        transfer::share_object(Holder { id: object::new(ctx) });
    }

    public fun df_add(holder: &mut Holder, name: u64, value: u64) {
        if (!dynamic_field::exists_(&holder.id, name)) {
            dynamic_field::add(&mut holder.id, name, value);
        }
    }

    public fun df_set(holder: &mut Holder, name: u64, value: u64) {
        if (dynamic_field::exists_with_type<u64, u64>(&holder.id, name)) {
            let stored = dynamic_field::borrow_mut<u64, u64>(&mut holder.id, name);
            *stored = value;
        }
    }

    public fun df_remove(holder: &mut Holder, name: u64) {
        if (dynamic_field::exists_with_type<u64, u64>(&holder.id, name)) {
            let _: u64 = dynamic_field::remove(&mut holder.id, name);
        }
    }

    public fun dof_add(holder: &mut Holder, name: u64, value: u64, ctx: &mut TxContext) {
        if (!dynamic_object_field::exists_(&holder.id, name)) {
            dynamic_object_field::add(&mut holder.id, name, Child { id: object::new(ctx), value });
        }
    }

    public fun dof_remove(holder: &mut Holder, name: u64) {
        if (dynamic_object_field::exists_with_type<u64, Child>(&holder.id, name)) {
            let Child { id, value: _ } = dynamic_object_field::remove<u64, Child>(&mut holder.id, name);
            object::delete(id);
        }
    }
}

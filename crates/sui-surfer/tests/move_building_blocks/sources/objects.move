// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module move_building_blocks::objects {
    use sui::object::UID;
    use std::option::Option;
    use sui::table::Table;
    use sui::tx_context::TxContext;
    use sui::object;
    use std::option;
    use sui::table;
    use sui::transfer;
    use sui::tx_context;

    struct Object has key, store {
        id: UID,
        wrapped: Option<Child>,
        table: Table<u8, Child>,
    }

    struct Child has key, store {
        id: UID,
    }

    public fun create_owned_object(ctx: &mut TxContext)  {
        let object = new_object(ctx);
        transfer::transfer(object, tx_context::sender(ctx));
    }

    public fun create_shared_object(ctx: &mut TxContext) {
        let object = new_object(ctx);
        transfer::share_object(object);
    }

    public fun freeze_object(object: Object) {
        transfer::freeze_object(object);
    }

    public fun create_owned_child(ctx: &mut TxContext) {
        let child = new_child(ctx);
        transfer::transfer(child, tx_context::sender(ctx));
    }

    public fun create_owned_children(count: u8, ctx: &mut TxContext) {
        let i = 0;
        while (i < count) {
            create_owned_child(ctx);
            i = i + 1;
        }
    }

    public fun wrap_child(object: &mut Object, child: Child) {
        unwrap_child(object);
        option::fill(&mut object.wrapped, child);
    }

    public fun unwrap_child(object: &mut Object) {
        if (option::is_some(&object.wrapped)) {
            let old_child = option::extract(&mut object.wrapped);
            let index = ((table::length(&object.table) + 1) as u8);
            table::add(&mut object.table, index, old_child);
        }
    }

    public fun unwrap_and_burn_child(object: &mut Object) {
        if (option::is_some(&object.wrapped)) {
            let old_child = option::extract(&mut object.wrapped);
            burn_child(old_child)
        }
    }

    public fun unwrap_and_share_child(object: &mut Object) {
        if (option::is_some(&object.wrapped)) {
            let old_child = option::extract(&mut object.wrapped);
            // The following operation will fail.
            transfer::share_object(old_child);
        }
    }

    public fun table_contains_child(object: &Object, index: u8) {
        let _ = table::contains(&object.table, index);
    }

    public fun table_add_child(object: &mut Object, child: Child, index: u8, ctx: &TxContext) {
        table_remove_child(object, index, ctx);
        table::add(&mut object.table, index, child);
    }

    public fun table_remove_child(object: &mut Object, index: u8, ctx: &TxContext) {
        if (table::contains(&object.table, index)) {
            let child = table::remove(&mut object.table, index);
            transfer::transfer(child, tx_context::sender(ctx));
        }
    }

    public fun table_burn_child(object: &mut Object, index: u8, dice: u8) {
        if (table::contains(&object.table, index) && dice % 5 == 0) {
            let child = table::remove(&mut object.table, index);
            burn_child(child);
        }
    }

    fun new_object(ctx: &mut TxContext): Object {
        Object {
            id: object::new(ctx),
            wrapped: option::none(),
            table: table::new(ctx),
        }
    }

    fun new_child(ctx: &mut TxContext): Child {
        Child {
            id:object::new(ctx),
        }
    }

    fun burn_child(child: Child) {
        let Child { id } = child;
        object::delete(id);
    }
}

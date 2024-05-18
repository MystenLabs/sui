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

    public struct Object has key, store {
        id: UID,
        wrapped: Option<Child>,
        table: Table<u8, Child>,
    }

    public struct Child has key, store {
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
        let mut i = 0;
        while (i < count) {
            create_owned_child(ctx);
            i = i + 1;
        }
    }

    public fun create_and_wrap_child(object: &mut Object, delete_old_child: bool, ctx: &mut TxContext) {
        let child = new_child(ctx);
        wrap_child(object, child, delete_old_child, ctx);
    }

    public fun wrap_child(object: &mut Object, child: Child, delete_old_child: bool, ctx: &TxContext) {
        if (delete_old_child) {
            unwrap_and_delete_child(object)
        } else {
            unwrap_child(object, ctx)
        };
        option::fill(&mut object.wrapped, child);
    }

    public fun unwrap_child(object: &mut Object, ctx: &TxContext) {
        if (option::is_some(&object.wrapped)) {
            let old_child = option::extract(&mut object.wrapped);
            transfer::transfer(old_child, tx_context::sender(ctx));
        }
    }

    public fun unwrap_child_and_add_to_table(object: &mut Object) {
        if (option::is_some(&object.wrapped)) {
            let old_child = option::extract(&mut object.wrapped);
            let index = ((table::length(&object.table) + 1) as u8);
            table::add(&mut object.table, index, old_child);
        }
    }

    public fun unwrap_and_delete_child(object: &mut Object) {
        if (option::is_some(&object.wrapped)) {
            let old_child = option::extract(&mut object.wrapped);
            delete_child(old_child)
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

    public fun table_delete_child(object: &mut Object, index: u8, dice: u8) {
        if (table::contains(&object.table, index) && dice % 5 == 0) {
            let child = table::remove(&mut object.table, index);
            delete_child(child);
        }
    }
    
    public fun delete(object: Object) {
        let Object { id, mut wrapped, table } = object;
        object::delete(id);
        if (option::is_some(&wrapped)) {
            let child = option::extract(&mut wrapped);
            delete_child(child);
        };
        option::destroy_none(wrapped);
        table::destroy_empty(table);
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

    fun delete_child(child: Child) {
        let Child { id } = child;
        object::delete(id);
    }
}

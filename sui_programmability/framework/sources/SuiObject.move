// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module Sui::SuiObject {
    use Sui::ID::{Self, VersionedID};
    use Sui::Transfer::{Self, ChildRef};
    use Sui::TxContext::{Self, TxContext};

    use Std::Vector;

    struct SuiObject<T: store> has key {
        id: VersionedID,
        children: vector<ChildRef>,
        data: T,
    }

    public fun create<T: store>(data: T, ctx: &mut TxContext): SuiObject<T> {
        SuiObject {
            id: TxContext::new_id(ctx),
            data,
        }
    }

    public fun create_with_child<T: store, C: key>(
        data: T, child: C, ctx: &mut TxContext
    ): SuiObject<T> {
        let parent = SuiObject::create(data, ctx);
        let child_ref Transfer::transfer_to_object(child, parent);
        Vector::push_back(&mut parent.children, child_ref)
    }

    public fun borrow<T: store>(object: &SuiObject<T>): &T {
        &object.data
    }

    public fun borrow_mut<T: store>(object: &mut SuiObject<T>): &mut T {
        &mut object.data
    }

    public fun unpack<T: store>(object: SuiObject<T>): (T, vector<ChildRef>) {
        let SuiObject {id, children, data} = object;
        ID::delete(id);
        (data, children)
    }

    public fun unpack_with_child<T: store>(object: SuiObject<T>): (T, ChildRef) {
        let SuiObject {id, children, data} = object;
        ID::delete(id);
        let child = Vector::pop_back(&mut children);
        Vector::destroy_empty(children);
        (data, child)
    }
}
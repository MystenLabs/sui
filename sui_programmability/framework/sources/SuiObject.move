// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module Sui::SuiObject {
    use Sui::ID::{Self, VersionedID};
    use Sui::TxContext::{Self, TxContext};

    struct SuiObject<T: store> has key {
        id: VersionedID,
        data: T,
    }

    public fun create<T: store>(data: T, ctx: &mut TxContext): SuiObject<T> {
        SuiObject {
            id: TxContext::new_id(ctx),
            data,
        }
    }

    public fun borrow<T: store>(object: &SuiObject<T>): &T {
        &object.data
    }

    public fun borrow_mut<T: store>(object: &mut SuiObject<T>): &mut T {
        &mut object.data
    }

    public fun unpack<T: store>(object: SuiObject<T>): T {
        let SuiObject {id, data} = object;
        ID::delete(id);
        data
    }
}
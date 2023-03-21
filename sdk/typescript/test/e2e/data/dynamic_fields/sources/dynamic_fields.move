// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module dynamic_fields::dynamic_fields_test {
    use sui::dynamic_field as dfield;
    use sui::dynamic_object_field as dof;
    use sui::object::{Self, UID};
    use sui::tx_context::{Self, TxContext};
    use sui::transfer;

    struct Test has key {
        id: UID,
    }

    struct Test1 has key, store {
        id: UID,
    }

    struct Test2 has key, store {
        id: UID,
    }

    fun init(ctx: &mut TxContext) {
        let test = Test{
            id: object::new(ctx),
        };

        let test1 =  Test1{
            id: object::new(ctx)
        };

        let test2 =  Test2{
            id: object::new(ctx)
        };

        dfield::add(&mut test.id, object::id(&test1), test1);

        dof::add(&mut test.id, object::id(&test2), test2);

        transfer::transfer(test, tx_context::sender(ctx))
    }
}

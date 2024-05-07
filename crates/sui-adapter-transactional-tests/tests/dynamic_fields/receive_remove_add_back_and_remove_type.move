// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// This test attempts to receive two objects, remove a child, add it back, remove it again, and then transfer/delete it.
// This is an interesting test case because when child objects are removed, added back and removed again,
// they won't show up in the child_object_effects in the object runtiem. We must look at either transfers
// or deleted_object_ids to figure them out.

//# init --addresses test=0x0 --accounts A

//# publish
module test::m1 {
    use sui::transfer::Receiving;

    public struct Object has key, store {
        id: UID,
    }

    public struct C1 has key, store {
        id: UID,
    }

    public struct C2 has key, store {
        id: UID,
    }

    public struct Wrapper<T: store> has key, store {
        id: UID,
        value: T,
    }

    public entry fun create(ctx: &mut TxContext) {
        let o = Object { id: object::new(ctx) };
        let c1 = C1 { id: object::new(ctx) };
        let c2 = C2 { id: object::new(ctx) };
        let o_address = object::id_address(&o);
        transfer::public_transfer(o, tx_context::sender(ctx));
        transfer::public_transfer(c1, o_address);
        transfer::public_transfer(c2, o_address);
    }

    public entry fun test_dof(parent: &mut Object, c1: Receiving<C1>, c2: Receiving<C2>) {
        let c1 = transfer::receive(&mut parent.id, c1);
        sui::dynamic_object_field::add(&mut parent.id, 0, c1);
        let c1: C1 = sui::dynamic_object_field::remove(&mut parent.id, 0);
        transfer::public_transfer(c1, @test);

        let c2 = transfer::receive(&mut parent.id, c2);
        sui::dynamic_object_field::add(&mut parent.id, 0, c2);
        let C2 { id } = sui::dynamic_object_field::remove(&mut parent.id, 0);
        object::delete(id);
    }

    public entry fun test_df(parent: &mut Object, c1: Receiving<C1>, c2: Receiving<C2>) {
        let c1 = transfer::receive(&mut parent.id, c1);
        sui::dynamic_field::add(&mut parent.id, 0, c1);
        let c1: C1 = sui::dynamic_field::remove(&mut parent.id, 0);
        transfer::public_transfer(c1, @test);

        let c2 = transfer::receive(&mut parent.id, c2);
        sui::dynamic_field::add(&mut parent.id, 0, c2);
        let C2 { id } = sui::dynamic_field::remove(&mut parent.id, 0);
        object::delete(id);
    }

    // Try to "wash" the receiving object through a dynamic object field and then wrap it in a wrapper object.
    public entry fun test_dof_wrapper(parent: &mut Object, c1: Receiving<C1>, _c2: Receiving<C2>, ctx: &mut TxContext) {
        let c1 = transfer::receive(&mut parent.id, c1);
        sui::dynamic_object_field::add(&mut parent.id, 0, c1);
        let c1: C1 = sui::dynamic_object_field::remove(&mut parent.id, 0);
        let w = Wrapper { id: object::new(ctx), value: c1 };
        sui::dynamic_object_field::add(&mut parent.id, 0, w);
        let w: Wrapper<C1> = sui::dynamic_object_field::remove(&mut parent.id, 0);
        transfer::public_transfer(w, @test);
    }

    // Try to "wash" the receiving object through a dynamic field and then wrap it in a wrapper object.
    public entry fun test_df_wrapper(parent: &mut Object, c1: Receiving<C1>, _c2: Receiving<C2>, ctx: &mut TxContext) {
        let c1 = transfer::receive(&mut parent.id, c1);
        sui::dynamic_field::add(&mut parent.id, 0, c1);
        let c1: C1 = sui::dynamic_field::remove(&mut parent.id, 0);
        let w = Wrapper { id: object::new(ctx), value: c1 };
        sui::dynamic_field::add(&mut parent.id, 0, w);
        let w: Wrapper<C1> = sui::dynamic_field::remove(&mut parent.id, 0);
        transfer::public_transfer(w, @test);
    }
}

//# run test::m1::create --sender A

//# run test::m1::test_dof --args object(2,2) receiving(2,0) receiving(2,1) --sender A

//# run test::m1::create --sender A

//# run test::m1::test_df --args object(4,2) receiving(4,0) receiving(4,1) --sender A

//# run test::m1::create --sender A

//# run test::m1::test_dof_wrapper --args object(6,2) receiving(6,0) receiving(6,1) --sender A

//# run test::m1::create --sender A

//# run test::m1::test_df_wrapper --args object(8,2) receiving(8,0) receiving(8,1) --sender A

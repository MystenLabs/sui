// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses tto=0x0 --accounts A

//# publish
module tto::M1 {
    use sui::transfer::Receiving;
    use sui::dynamic_object_field as dof;
    use sui::dynamic_field as df;

    const KEY: u64 = 0;
    const BKEY: u64 = 1;

    public struct A has key, store {
        id: UID,
        value: vector<u8>,
    }

    public struct Wrapper<T: key + store> has key, store {
        id: UID,
        value: T,
    }

    public fun start(ctx: &mut TxContext) {
        let a_parent = A { id: object::new(ctx), value: b"a_parent" };

        let mut b_parent = A { id: object::new(ctx), value: b"b_parent" };
        let mut b_child = A { id: object::new(ctx), value: b"b_child" };
        let b_child_child = A { id: object::new(ctx), value: b"b_child_child" };
        let b_child_child_df = A { id: object::new(ctx), value: b"b_child_child_df" };

        let wrapped_dof = A { id: object::new(ctx), value: b"wrapped_df" };
        let mut to_wrap = A { id: object::new(ctx), value: b"wrapped" };
        dof::add(&mut to_wrap.id, KEY, wrapped_dof);
        let wrapped = Wrapper { id: object::new(ctx), value: to_wrap };

        dof::add(&mut b_child.id, KEY, b_child_child);
        df::add(&mut b_child.id, BKEY, b_child_child_df);
        dof::add(&mut b_parent.id, KEY, b_child);

        let a_address = object::id_address(&a_parent);
        transfer::public_transfer(a_parent, tx_context::sender(ctx));
        transfer::public_transfer(b_parent, a_address);
        transfer::public_transfer(wrapped, a_address);
    }

    public entry fun receive_b_parent(a_parent: &mut A, x: Receiving<A>) {
        let b_parent = transfer::receive(&mut a_parent.id, x);
        dof::add(&mut a_parent.id, KEY, b_parent);
        let b_parent: &A = dof::borrow(&a_parent.id, KEY);
        let b_child: &A = dof::borrow(&b_parent.id, KEY);
        let b_child_child: &A = dof::borrow(&b_child.id, KEY);
        let b_child_child_df: &A = df::borrow(&b_child.id, BKEY);
        assert!(a_parent.value == b"a_parent", 0);
        assert!(b_child.value == b"b_child", 1);
        assert!(b_parent.value == b"b_parent", 2);
        assert!(b_child_child.value == b"b_child_child", 3);
        assert!(b_child_child_df.value == b"b_child_child_df", 4);
    }

    public entry fun receive_wrapped(a_parent: &mut A, x: Receiving<Wrapper<A>>) {
        let wrapped = transfer::receive(&mut a_parent.id, x);
        dof::add(&mut a_parent.id, BKEY, wrapped);
        let wrapped: &Wrapper<A> = dof::borrow(&a_parent.id, BKEY);
        let wrapped_df: &A = dof::borrow(&wrapped.value.id, KEY);
        assert!(wrapped.value.value == b"wrapped", 5);
        assert!(wrapped_df.value == b"wrapped_df", 6);
    }
}

// receive, add, and then access through parent.
// * A dynamic object field
// * A dynamic object field of a dynamic object field
// * A dynamic field of a dynamic object field
// * A dynamic object field of wrapped object that was received

//# run tto::M1::start --sender A

//# view-object 2,0

//# view-object 2,1

//# view-object 2,2

//# view-object 2,3

//# view-object 2,4

//# view-object 2,5

//# view-object 2,6

//# view-object 2,7

//# view-object 2,8

//# view-object 2,9

// E_OBJECT_TYPE_MISMATCH
// Try to load an invalid type that will cause indexing to fail.
//# run tto::M1::receive_b_parent --args object(2,8) receiving(2,9) --sender A

//# run tto::M1::receive_b_parent --args object(2,8) receiving(2,6) --sender A

//# run tto::M1::receive_wrapped --args object(2,8) receiving(2,9) --sender A

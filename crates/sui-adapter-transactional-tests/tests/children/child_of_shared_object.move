// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses t1=0x0 t2=0x0 t3=0x0 --accounts A

//# publish

module t3::o3 {
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    struct Obj3 has key, store {
        id: UID,
    }

    public entry fun create(ctx: &mut TxContext) {
        let o = Obj3 { id: object::new(ctx) };
        transfer::transfer(o, tx_context::sender(ctx))
    }
}

//# publish --dependencies t3

module t2::o2 {
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};
    use t3::o3::Obj3;

    struct Obj2 has key, store {
        id: UID,
    }

    public entry fun create_shared(child: Obj3, ctx: &mut TxContext) {
        transfer::share_object(new(child, ctx))
    }

    public entry fun create_owned(child: Obj3, ctx: &mut TxContext) {
        transfer::transfer(new(child, ctx), tx_context::sender(ctx))
    }

    public entry fun use_o2_o3(_o2: &mut Obj2, o3: Obj3, ctx: &mut TxContext) {
        transfer::transfer(o3, tx_context::sender(ctx))
    }

    fun new(child: Obj3, ctx: &mut TxContext): Obj2 {
        let id = object::new(ctx);
        sui::dynamic_object_field::add(&mut id, 0, child);
        Obj2 { id }
    }
}


//# publish --dependencies t2 t3

module t1::o1 {
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};
    use t2::o2::Obj2;
    use t3::o3::Obj3;

    struct Obj1 has key {
        id: UID,
    }

    public entry fun create_shared(child: Obj2, ctx: &mut TxContext) {
        transfer::share_object(new(child, ctx))
    }

    // This function will be invalid if _o2 is a shared object and owns _o3.
    public entry fun use_o2_o3(_o2: &mut Obj2, o3: Obj3, ctx: &mut TxContext) {
        transfer::transfer(o3, tx_context::sender(ctx))
    }

    fun new(child: Obj2, ctx: &mut TxContext): Obj1 {
        let id = object::new(ctx);
        sui::dynamic_object_field::add(&mut id, 0, child);
        Obj1 { id }
    }
}

//# run t3::o3::create --sender A

//# run t2::o2::create_shared --args object(111) --sender A

// child arguments cannot be taken directly
//# run t1::o1::use_o2_o3 --args object(113) object(114) --sender A

//# run t2::o2::use_o2_o3 --args object(113) object(110) --sender A

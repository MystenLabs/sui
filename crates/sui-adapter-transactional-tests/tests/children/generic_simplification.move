// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses t1=0x0 t2=0x0 t3=0x0 A=0x42

//# publish

module t1::marketplace {
    use sui::object::{Self, UID};
    use sui::tx_context::{Self, TxContext};
    use sui::transfer;

    struct Marketplace<phantom T: key + store> has key {
        id: UID,
    }

    struct Listing<T: key + store> has key {
        id: UID,
        item: T
    }

    public entry fun create<T: key + store>(ctx: &mut TxContext) {
        let o = Marketplace<T> { id: object::new(ctx) };
        transfer::share_object(o)
    }

    public entry fun list<T: key + store>(m: &mut Marketplace<T>, item: T, ctx: &mut TxContext) {
        transfer::transfer_to_object(Listing { id: object::new(ctx), item }, m)
    }

    public entry fun delist<T: key + store>(_m: &mut Marketplace<T>, listing: Listing<T>, ctx: &mut TxContext) {
        let Listing { id, item } = listing;
        object::delete(id);
        transfer::transfer(item, tx_context::sender(ctx))
    }
}

//# publish

module t2::items {
    use sui::object::{Self, UID};
    use sui::tx_context::{TxContext};
    use t1::marketplace::{Self, Marketplace};

    struct Item has key, store {
        id: UID
    }

    public entry fun create(ctx: &mut TxContext) {
        marketplace::create<Item>(ctx);
    }

    public entry fun list(m: &mut Marketplace<Item>, ctx: &mut TxContext) {
        marketplace::list<Item>(m, Item { id: object::new(ctx) }, ctx);
    }
}


//# run t2::items::create

//# run t2::items::list --args object(107)

//# run t1::marketplace::delist --args object(107) object(109) --type-args t2::items::Item



// //# run t1::o2::create_shared --args object(109)

// // This run should error as Obj2/Obj3 were not defined in o1
// //# run t1::o1::use_o2_o3 --args object(111) object(109)

// //# run t2::o2::use_o2_o3 --args object(111) object(109)

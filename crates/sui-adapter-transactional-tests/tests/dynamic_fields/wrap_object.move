// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// test wrapping an object in a dynamic field

//# init --addresses a=0x0 --accounts A

//# publish
module a::m {

use sui::dynamic_field;
use sui::dynamic_object_field;
use sui::object;
use sui::tx_context::{sender, TxContext};

struct Obj has key, store {
    id: object::UID,
}

entry fun mint(ctx: &mut TxContext) {
    let parent = object::new(ctx);
    dynamic_object_field::add(&mut parent, 0, Obj { id: object::new(ctx) });
    sui::transfer::transfer(Obj { id: parent }, sender(ctx))
}

entry fun take_and_wrap(obj: &mut Obj) {
    let v = dynamic_object_field::remove<u64, Obj>(&mut obj.id, 0);
    dynamic_field::add(&mut obj.id, 0, v)
}

entry fun take_and_destroy(obj: &mut Obj) {
    let Obj { id } = dynamic_object_field::remove(&mut obj.id, 0);
    object::delete(id)
}

entry fun take_and_take(obj: &mut Obj, ctx: &mut TxContext) {
    let v = dynamic_object_field::remove<u64, Obj>(&mut obj.id, 0);
    sui::transfer::transfer(v, sender(ctx))
}

}

//# run a::m::mint --sender A

//# view-object 107

//# run a::m::take_and_wrap --sender A --args object(106)


//# run a::m::mint --sender A

//# view-object 112

//# run a::m::take_and_destroy --sender A --args object(113)


//# run a::m::mint --sender A

//# view-object 118

//# run a::m::take_and_take --sender A --args object(117)

//# view-object 118

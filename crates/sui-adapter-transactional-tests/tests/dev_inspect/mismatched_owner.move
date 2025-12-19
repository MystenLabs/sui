// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// check that owner can be mismatched in dev inspect

//# init --addresses test=0x0 --accounts A B

//# publish
module test::m;

public struct Obj has key, store {
    id: UID,
}

public fun make_owned(ctx: &mut TxContext) {
    transfer::public_transfer(Obj { id: object::new(ctx) }, ctx.sender())
}

public fun make_shared(ctx: &mut TxContext) {
    transfer::public_share_object(Obj { id: object::new(ctx) })
}

public fun make_party(ctx: &mut TxContext) {
    transfer::public_party_transfer(
        Obj { id: object::new(ctx) },
        sui::party::single_owner(ctx.sender()),
    )
}

public fun make_immutable(ctx: &mut TxContext) {
    transfer::public_freeze_object(Obj { id: object::new(ctx) });
}

public fun make_object_owned(parent: &Obj, ctx: &mut TxContext) {
    transfer::public_transfer(Obj { id: object::new(ctx) }, parent.id.to_address());
}

public fun obj_mut(_: &mut Obj) {
}

public fun obj_imm(_: &Obj) {
}

public fun drop_receiving(_: sui::transfer::Receiving<Obj>) {
}

//# programmable --sender A
//> test::m::make_owned();

//# programmable --sender A
//> test::m::make_shared();

//# programmable --sender A
//> test::m::make_party();

//# programmable --sender A
//> test::m::make_immutable();

//# programmable --sender A --inputs object(2,0)
//> test::m::make_object_owned(Input(0));

//# programmable --sender A --inputs owned(3,0) --dev-inspect
// use shared as owned
//> test::m::obj_mut(Input(0));

//# programmable --sender A --inputs owned(4,0) --dev-inspect
// use party as owned
//> test::m::obj_mut(Input(0))

//# programmable --sender A --inputs owned(6,0) --dev-inspect
// use object owned as owned
//> test::m::obj_mut(Input(0))

//# programmable --sender A --inputs receiving(3,0) --dev-inspect
// use shared as receiving
//> test::m::drop_receiving(Input(0))

//# programmable --sender A --inputs receiving(4,0) --dev-inspect
// use party as receiving
//> test::m::drop_receiving(Input(0))

//# programmable --sender A --inputs receiving(5,0) --dev-inspect
// use imm as receiving
//> test::m::drop_receiving(Input(0))

//# programmable --sender A --inputs mutshared(2,0) --dev-inspect
// use owned as shared
//> test::m::obj_mut(Input(0));

//# programmable --sender A --inputs mutshared(5,0) --dev-inspect
// use imm as shared
//> test::m::obj_mut(Input(0));

//# programmable --sender A --inputs mutshared(6,0) --dev-inspect
// use object owned as shared
//> test::m::obj_mut(Input(0));

//# programmable --sender A --inputs immshared(2,0) --dev-inspect
// use owned as imm shared
//> test::m::obj_mut(Input(0));

//# programmable --sender A --inputs immshared(5,0) --dev-inspect
// use imm as imm shared
//> test::m::obj_mut(Input(0));

//# programmable --sender A --inputs immshared(6,0) --dev-inspect
// use object owned as imm shared
//> test::m::obj_mut(Input(0));

//# programmable --sender A --inputs nonexclusive(2,0) --dev-inspect
// use owned as non-exclusive write
//> test::m::obj_mut(Input(0));

//# programmable --sender A --inputs nonexclusive(3,0) --dev-inspect
// use shared as non-exclusive write
//> test::m::obj_mut(Input(0));

//# programmable --sender A --inputs nonexclusive(4,0) --dev-inspect
// use party as non-exclusive write
//> test::m::obj_mut(Input(0));

//# programmable --sender A --inputs nonexclusive(5,0) --dev-inspect
// use imm as non-exclusive write
//> test::m::obj_mut(Input(0));

//# programmable --sender A --inputs nonexclusive(6,0) --dev-inspect
// use object owned as non-exclusive write
//> test::m::obj_mut(Input(0));

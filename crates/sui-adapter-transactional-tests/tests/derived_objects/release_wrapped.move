// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Release and re-claim a derived object that is wrapped, either as an object or as a bare UID.

//# init --addresses ex=0x0 --accounts A

//# publish
module ex::m;

use sui::derived_object;

public struct Parent has key { id: UID }

public struct Obj has key, store { id: UID }

public struct Wrapper has key { id: UID, inner: Obj }

public struct UidHolder has key { id: UID, uid: UID }

public fun create_parent(ctx: &mut TxContext) {
    transfer::transfer(Parent { id: object::new(ctx) }, ctx.sender())
}

public fun claim(parent: &mut Parent): Obj {
    Obj { id: derived_object::claim(&mut parent.id, 0u64) }
}

public fun release(parent: &mut Parent, obj: Obj) {
    let Obj { id } = obj;
    derived_object::release(&mut parent.id, id)
}

public fun wrap(inner: Obj, ctx: &mut TxContext) {
    transfer::transfer(Wrapper { id: object::new(ctx), inner }, ctx.sender())
}

public fun unwrap(wrapper: Wrapper): Obj {
    let Wrapper { id, inner } = wrapper;
    id.delete();
    inner
}

public fun claim_into_holder(parent: &mut Parent, ctx: &mut TxContext) {
    let uid = derived_object::claim(&mut parent.id, 0u64);
    transfer::transfer(UidHolder { id: object::new(ctx), uid }, ctx.sender())
}

public fun release_from_holder(parent: &mut Parent, holder: UidHolder) {
    let UidHolder { id, uid } = holder;
    id.delete();
    derived_object::release(&mut parent.id, uid)
}

//# run ex::m::create_parent --sender A

//# programmable --sender A --inputs object(2,0) @A
//> 0: ex::m::claim(Input(0));
//> TransferObjects([Result(0)], Input(1))

// X is wrapped.
//# run ex::m::wrap --sender A --args object(3,0)

// Unwrap and release: X is unwrapped_then_deleted.
//# programmable --sender A --inputs object(2,0) object(4,0)
//> 0: ex::m::unwrap(Input(1));
//> ex::m::release(Input(0), Result(0))

// Claim X again in a later transaction: X is created.
//# programmable --sender A --inputs object(2,0) @A
//> 0: ex::m::claim(Input(0));
//> TransferObjects([Result(0)], Input(1))

//# view-object 3,0

//# run ex::m::wrap --sender A --args object(3,0)

// Unwrap, release, claim, transfer in one transaction: X is unwrapped.
//# programmable --sender A --inputs object(2,0) object(8,0) @A
//> 0: ex::m::unwrap(Input(1));
//> ex::m::release(Input(0), Result(0));
//> 2: ex::m::claim(Input(0));
//> TransferObjects([Result(2)], Input(2))

//# view-object 3,0

//# run ex::m::wrap --sender A --args object(3,0)

// Unwrap, release, claim, re-wrap in one transaction: X stays wrapped and is not in the effects.
//# programmable --sender A --inputs object(2,0) object(11,0)
//> 0: ex::m::unwrap(Input(1));
//> ex::m::release(Input(0), Result(0));
//> 2: ex::m::claim(Input(0));
//> ex::m::wrap(Result(2))

//# programmable --sender A --inputs object(2,0) object(12,0)
//> 0: ex::m::unwrap(Input(1));
//> ex::m::release(Input(0), Result(0))

// X is claimed as a bare UID inside a holder and never becomes an object, so X is not in the
// effects.
//# run ex::m::claim_into_holder --sender A --args object(2,0)

// Releasing the bare UID: X is unwrapped_then_deleted.
//# run ex::m::release_from_holder --sender A --args object(2,0) object(14,0)

//# run ex::m::claim_into_holder --sender A --args object(2,0)

//# run ex::m::release_from_holder --sender A --args object(2,0) object(16,0)

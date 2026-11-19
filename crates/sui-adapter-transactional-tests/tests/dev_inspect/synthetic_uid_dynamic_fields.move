// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A dev-inspect value built from bytes reads its dynamic fields at the version before the
// transaction's lamport version, so it sees only fields no newer than the transaction's inputs.

//# init --addresses test=0x0 s=0x0 --accounts A

//# publish
module test::m;

use sui::dynamic_field as df;

public struct S has key, store {
    id: UID,
    value: u64,
}

public fun create(ctx: &mut TxContext) {
    transfer::transfer(S { id: object::new(ctx), value: 0 }, ctx.sender())
}

public fun add_field(s: &mut S) {
    df::add(&mut s.id, 0u64, 0u64)
}

public fun assert_has_field(s: &S) {
    assert!(df::exists(&s.id, 0u64), 0);
}

public fun assert_no_field(s: &S) {
    assert!(!df::exists(&s.id, 0u64), 1);
}

public fun bump_field(s: &mut S) {
    let f: &mut u64 = df::borrow_mut(&mut s.id, 0u64);
    *f = *f + 1;
}

public fun remove_field(s: &mut S) {
    let _: u64 = df::remove(&mut s.id, 0u64);
}

//# run test::m::create --sender A

//# set-address s object(2,0)

//# run test::m::add_field --sender A --args object(2,0)

//# programmable --sender A --inputs object(2,0)@2 --dev-inspect
//> 0: test::m::assert_no_field(Input(0));

//# programmable --sender A --inputs object(2,0) --dev-inspect
//> 0: test::m::assert_has_field(Input(0));

// A newer object, to raise the lamport version of the dev inspects below

//# run test::m::create --sender A

// The same object built from bytes. The lamport version comes the inputs, and the only
// one is the mock gas coin.

//# programmable --sender A --inputs struct(@s,0) --dev-inspect
//> 0: test::m::assert_no_field(Input(0));

//# programmable --sender A --inputs struct(@s,0) --dev-inspect
//> 0: test::m::remove_field(Input(0));

// With the newer object as an input, the field is visible and can be written.

//# programmable --sender A --inputs struct(@s,0) object(7,0) --dev-inspect
//> 0: test::m::assert_has_field(Input(0));

//# programmable --sender A --inputs struct(@s,0) object(7,0) --dev-inspect
//> 0: test::m::bump_field(Input(0));

//# programmable --sender A --inputs struct(@s,0) object(7,0) --dev-inspect
//> 0: test::m::remove_field(Input(0));

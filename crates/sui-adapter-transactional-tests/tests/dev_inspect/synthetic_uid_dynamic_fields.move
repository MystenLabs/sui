// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A dev-inspect value built from bytes reads its dynamic fields at the latest version, whereas an
// old version of the same object cannot see fields added after it.

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

//# run test::m::create --sender A

//# set-address s object(2,0)

//# run test::m::add_field --sender A --args object(2,0)

//# programmable --sender A --inputs object(2,0)@2 --dev-inspect
//> 0: test::m::assert_no_field(Input(0));

//# programmable --sender A --inputs object(2,0) --dev-inspect
//> 0: test::m::assert_has_field(Input(0));

// The same object built from bytes

//# programmable --sender A --inputs struct(@s,0) --dev-inspect
//> 0: test::m::assert_has_field(Input(0));

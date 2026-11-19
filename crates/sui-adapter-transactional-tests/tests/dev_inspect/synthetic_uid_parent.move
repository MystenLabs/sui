// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Dev-inspect can build objects from BCS bytes. Their UIDs, including wrapped ones, can be dynamic
// field parents.

//# init --addresses test=0x0 --accounts A

//# publish
module test::m;

use sui::dynamic_field as df;
use sui::dynamic_object_field as dof;

public struct S has key, store {
    id: UID,
    value: u64,
}

public struct Outer has key {
    id: UID,
    inner: S,
}

public fun has_field(s: &S): bool {
    df::exists(&s.id, 0u64)
}

public fun add_field(s: &mut S) {
    df::add(&mut s.id, 0u64, 0u64)
}

public fun add_object_field(s: &mut S, ctx: &mut TxContext) {
    dof::add(&mut s.id, 0u64, S { id: object::new(ctx), value: 0 })
}

public fun inner_add_field(o: &mut Outer) {
    df::add(&mut o.inner.id, 0u64, 0u64);
    assert!(df::exists(&o.inner.id, 0u64));
}

//# programmable --sender A --inputs struct(@0x1000000000000000000000000000000000000000000000000000000000001234,0) --dev-inspect
//> 0: test::m::has_field(Input(0));

//# programmable --sender A --inputs struct(@0x1000000000000000000000000000000000000000000000000000000000001234,0) --dev-inspect
//> 0: test::m::add_field(Input(0));

//# programmable --sender A --inputs struct(@0x1000000000000000000000000000000000000000000000000000000000001234,0) --dev-inspect
//> 0: test::m::add_object_field(Input(0));

//# programmable --sender A --inputs struct(@0x1000000000000000000000000000000000000000000000000000000000001234,struct(@0x1000000000000000000000000000000000000000000000000000000000005678,0)) --dev-inspect
//> 0: test::m::inner_add_field(Input(0));

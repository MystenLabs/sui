// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A public function's dependencies are pinned `at_least` and resolve to the
// version the package was published against unless another command in the same
// transaction pins them higher. `Dep::stamp` writes a per-version constant (1
// in v1, 2 in v2) into the shared `S`, so the observed `v` tells us which
// version of `Dep` actually ran behind `Root::go`.

//# init --addresses Dep_V1=0x0 Dep_V2=0x0 Root=0x0 --accounts A

//# publish --upgradeable --sender A
module Dep_V1::d;

public struct S has key, store { id: sui::object::UID, v: u64 }

public fun make(ctx: &mut sui::tx_context::TxContext) {
    sui::transfer::share_object(S { id: sui::object::new(ctx), v: 0 })
}

public fun stamp(s: &mut S) { s.v = 1 }

public fun ping() {}

//# publish --upgradeable --dependencies Dep_V1 --sender A
module Root::m;
use Dep_V1::d;

// Public: deps pinned `at_least`, so `Dep` may be upgraded under us.
public fun go(s: &mut d::S) { d::stamp(s) }

//# run Dep_V1::d::make --sender A

//# upgrade --package Dep_V1 --upgrade-capability 1,1 --sender A
module Dep_V2::d;

public struct S has key, store { id: sui::object::UID, v: u64 }

public fun make(ctx: &mut sui::tx_context::TxContext) {
    sui::transfer::share_object(S { id: sui::object::new(ctx), v: 0 })
}

public fun stamp(s: &mut S) { s.v = 2 }

public fun ping() {}

//# programmable --sender A --inputs object(3,0)
// Nothing else pins `Dep`, so `at_least(1)` resolves to v1 => writes 1.
//> Root::m::go(Input(0));

//# view-object 3,0

//# programmable --sender A --inputs object(3,0)
// `ping` pins `Dep` exact(2); `go`'s `at_least(1)` unifies up to exact(2) => `go` runs v2 => writes 2.
//> 0: Root::m::go(Input(0));
//> 1: Dep_V2::d::ping();

//# view-object 3,0

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A private/`entry` function's dependencies are pinned `exact`, so they are locked to the version the
// package was published against and cannot be upgraded under it. `Dep::stamp` writes a per-version
// constant (1 in v1, 2 in v2) into the shared `S`. Even though `Dep` is upgraded to v2, the entry
// call `Root::go` still runs v1 (writes 1); and a command that tries to pin `Dep` higher in the same
// transaction conflicts (InvalidLinkage) rather than upgrading it.

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

// Private `entry`: deps pinned `exact`, so `Dep` is locked to v1 under this call.
entry fun go(s: &mut d::S) { d::stamp(s) }

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
// Locked: entry call pins `Dep` exact(1) => runs v1 => writes 1, even though v2 exists.
//> Root::m::go(Input(0));

//# view-object 3,0

//# programmable --sender A --inputs object(3,0)
// `ping` pins `Dep` exact(2); `go` pins `Dep` exact(1) => exact/exact conflict => InvalidLinkage.
//> 0: Root::m::go(Input(0));
//> 1: Dep_V2::d::ping();

//# view-object 3,0

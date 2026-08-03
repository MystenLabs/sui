// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Visibility-based pinning applies to the transitive dependency closure, not just direct deps.
// `Root -> Mid -> Leaf`. `Leaf::val` returns a per-version constant (10 in v1, 20 in v2) which
// `Mid::mid_val` forwards and `Root` stores into its `R`. An entry call into `Root` pins the entire
// closure (including the transitive `Leaf`) `exact`, locking it to v1; a public call pins the closure
// `at_least`, so the transitive `Leaf` can be upgraded to v2 by another command in the transaction.

//# init --addresses Leaf_V1=0x0 Leaf_V2=0x0 Mid=0x0 Root=0x0 --accounts A

//# publish --upgradeable --sender A
module Leaf_V1::l;

public fun val(): u64 { 1 }

public fun ping() {}

//# publish --dependencies Leaf_V1 --sender A
module Mid::mid;
use Leaf_V1::l;

public fun mid_val(): u64 { l::val() }

//# publish --dependencies Mid Leaf_V1 --sender A
module Root::m;
use Mid::mid;

public struct R has key, store { id: sui::object::UID, v: u64 }

public fun make(ctx: &mut sui::tx_context::TxContext) {
    sui::transfer::share_object(R { id: sui::object::new(ctx), v: 0 })
}

// Public: transitive deps pinned `at_least`.
public fun pub_go(r: &mut R) { r.v = mid::mid_val() }

// Private `entry`: transitive deps pinned `exact`.
entry fun ent_go(r: &mut R) { r.v = mid::mid_val() }

//# run Root::m::make --sender A

//# upgrade --package Leaf_V1 --upgrade-capability 1,1 --sender A
module Leaf_V2::l;

public fun val(): u64 { 2 }

public fun ping() {}

//# programmable --sender A --inputs object(4,0)
// Entry locks the whole closure: transitive `Leaf` pinned exact(1) => runs v1 => writes 10.
//> Root::m::ent_go(Input(0));

//# view-object 4,0

//# programmable --sender A --inputs object(4,0)
// Public + `ping` pinning `Leaf` exact(2): transitive `Leaf` unifies up to v2 => writes 20.
//> 0: Root::m::pub_go(Input(0));
//> 1: Leaf_V2::l::ping();

//# view-object 4,0

//# programmable --sender A --inputs object(4,0)
// Entry pins transitive `Leaf` exact(1); `ping` pins it exact(2) => exact/exact conflict => InvalidLinkage.
//> 0: Root::m::ent_go(Input(0));
//> 1: Leaf_V2::l::ping();

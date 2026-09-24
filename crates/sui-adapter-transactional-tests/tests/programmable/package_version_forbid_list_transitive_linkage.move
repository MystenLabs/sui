// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A defining-ID type argument can lift a transitive executable dependency. The type-only
// reference remains allowed, but Root must be denied once it resolves Leaf to a forbidden version.

//# init --addresses LeafV1=0x0 LeafV2=0x0 LeafV3=0x0 Mid=0x0 Root=0x0 Generic=0x0 --accounts A

//# publish --upgradeable --sender A
module LeafV1::l;

public struct A has drop {}

public fun value(): u64 { 1 }

//# publish --dependencies LeafV1 --sender A
module Mid::m;
use LeafV1::l;

public fun value(): u64 { l::value() }

//# publish --dependencies Mid LeafV1 --sender A
module Root::r;
use Mid::m;

public struct R has key, store { id: sui::object::UID, v: u64 }

public fun make(ctx: &mut sui::tx_context::TxContext) {
    sui::transfer::share_object(R { id: sui::object::new(ctx), v: 0 })
}

public fun public_go(r: &mut R) { r.v = m::value() }

//# publish --sender A
module Generic::g;

public fun use_type<T>() {}

//# run Root::r::make --sender A

//# upgrade --package LeafV1 --upgrade-capability 1,1 --sender A
module LeafV2::l;

public struct A has drop {}

// B is introduced in v2, so its defining ID pins Leaf at_least(2).
public struct B has drop {}

public fun value(): u64 { 2 }

//# programmable --sender A --inputs object(5,0)
// The Leaf-v2 type argument lifts Root's transitive Leaf dependency to v2.
//> 0: Root::r::public_go(Input(0));
//> 1: Generic::g::use_type<LeafV2::l::B>();

//# view-object 5,0

// Make v2 historical so it can be forbidden.
//# upgrade --package LeafV2 --upgrade-capability 1,1 --sender A
module LeafV3::l;

public struct A has drop {}

public struct B has drop {}

public fun value(): u64 { 3 }

//# run sui::package_config::forbid_version --args object(0x426) object(1,1) 2 --sender A

// Leaf is still type-only here, so the forbidden version remains allowed.
//# run Generic::g::use_type --type-args LeafV2::l::B --sender A

//# programmable --sender A --inputs object(5,0)
// Without the type argument, Root's transitive Leaf dependency still resolves to v1.
//> Root::r::public_go(Input(0));

//# view-object 5,0

//# programmable --sender A --inputs object(5,0)
// The type argument lifts Root's executable transitive dependency to forbidden Leaf v2.
//> 0: Root::r::public_go(Input(0));
//> 1: Generic::g::use_type<LeafV2::l::B>();

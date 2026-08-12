// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Forbid-list checks use the version selected by unified linkage. Public dependencies can
// unify upward; defining-ID type arguments participate in that unification but remain allowed
// when they are the only use of a package.

//# init --addresses DepV1=0x0 DepV2=0x0 DepV3=0x0 Root=0x0 Generic=0x0 --accounts A

//# publish --upgradeable --sender A
module DepV1::d;

public struct S has key, store { id: sui::object::UID, v: u64 }

public struct A has drop {}

public fun make(ctx: &mut sui::tx_context::TxContext) {
    sui::transfer::share_object(S { id: sui::object::new(ctx), v: 0 })
}

public fun stamp(s: &mut S) { s.v = 1 }

public fun ping() {}

//# publish --dependencies DepV1 --sender A
module Root::r;
use DepV1::d;

// Public dependencies are pinned at_least.
public fun public_go(s: &mut d::S) { d::stamp(s) }

// Entry dependencies are pinned exact.
entry fun entry_go(s: &mut d::S) { d::stamp(s) }

//# publish --sender A
module Generic::g;

public fun use_type<T>() {}

//# run DepV1::d::make --sender A

//# upgrade --package DepV1 --upgrade-capability 1,1 --sender A
module DepV2::d;

public struct S has key, store { id: sui::object::UID, v: u64 }

public struct A has drop {}

// B is introduced in v2, so its defining ID pins Dep at_least(2).
public struct B has drop {}

public fun make(ctx: &mut sui::tx_context::TxContext) {
    sui::transfer::share_object(S { id: sui::object::new(ctx), v: 0 })
}

public fun stamp(s: &mut S) { s.v = 2 }

public fun ping() {}

// Forbid the version Root resolves when it is called alone.
//# run sui::package_config::forbid_version --args object(0x426) object(1,1) 1 --sender A

//# programmable --sender A --inputs object(4,0)
// Root's executable dependency resolves to forbidden Dep v1, so signing rejects this.
//> Root::r::public_go(Input(0));

//# programmable --sender A --inputs object(4,0)
// DepV2 pins Dep exact(2), lifting Root's at_least(1) dependency to allowed v2.
//> 0: Root::r::public_go(Input(0));
//> 1: DepV2::d::ping();

//# view-object 4,0

// Make v2 historical so it can be forbidden.
//# upgrade --package DepV2 --upgrade-capability 1,1 --sender A
module DepV3::d;

public struct S has key, store { id: sui::object::UID, v: u64 }

public struct A has drop {}

public struct B has drop {}

public fun make(ctx: &mut sui::tx_context::TxContext) {
    sui::transfer::share_object(S { id: sui::object::new(ctx), v: 0 })
}

public fun stamp(s: &mut S) { s.v = 3 }

public fun ping() {}

// Forbid v2 as well.
//# run sui::package_config::forbid_version --args object(0x426) object(1,1) 2 --sender A

// A forbidden type package remains allowed when no package code is executed from it.
//# run Generic::g::use_type --type-args DepV2::d::B --sender A

//# programmable --sender A --inputs object(4,0)
// The same defining-ID type argument lifts Root's executable dependency to forbidden v2.
//> 0: Root::r::public_go(Input(0));
//> 1: Generic::g::use_type<DepV2::d::B>();

//# programmable --sender A --inputs object(4,0)
// Referring to B through v3 still uses its v2 defining ID, so it has the same forbidden v2
// executable dependency.
//> 0: Root::r::public_go(Input(0));
//> 1: Generic::g::use_type<DepV3::d::B>();

//# programmable --sender A --inputs object(4,0)
// The direct v3 call raises Root's v1 and B's defining-ID v2 constraints to allowed v3.
//> 0: Root::r::public_go(Input(0));
//> 1: Generic::g::use_type<DepV3::d::B>();
//> 2: DepV3::d::ping();

//# view-object 4,0

//# programmable --sender A --inputs object(4,0)
// Linkage collection fails on the exact/exact conflict, so signing fails open and execution
// reports InvalidLinkage instead of TransactionDenied.
//> 0: Root::r::entry_go(Input(0));
//> 1: DepV2::d::ping();

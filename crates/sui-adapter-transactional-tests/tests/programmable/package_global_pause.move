// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Package global pause applies to every version in a package family and to executable transitive
// dependencies. Type-only references remain allowed. Signing observes pre-transaction state, so
// a PTB that enables the pause may still execute the package, while one that disables an existing
// pause is denied before the disable executes.

//# init --addresses LeafV1=0x0 LeafV2=0x0 Mid=0x0 Root=0x0 Generic=0x0 --accounts A

//# publish --upgradeable --sender A
module LeafV1::leaf;

public struct Type has drop {}

public fun value(): u64 { 1 }

public fun ping() {}

//# publish --dependencies LeafV1 --sender A
module Mid::mid;
use LeafV1::leaf;

public fun value(): u64 { leaf::value() }

//# publish --dependencies Mid LeafV1 --sender A
module Root::root;
use Mid::mid;

public struct State has key { id: sui::object::UID, value: u64 }

public fun make(ctx: &mut sui::tx_context::TxContext) {
    sui::transfer::share_object(State { id: sui::object::new(ctx), value: 0 })
}

public fun go(state: &mut State) { state.value = mid::value() }

//# publish --sender A
module Generic::generic;

public fun use_type<T>() {}

//# run Root::root::make --sender A

//# upgrade --package LeafV1 --upgrade-capability 1,1 --sender A
module LeafV2::leaf;

public struct Type has drop {}

public fun value(): u64 { 2 }

public fun ping() {}

// All package-family versions and transitive execution are allowed before pause.
//# run LeafV1::leaf::ping --sender A

//# run LeafV2::leaf::ping --sender A

//# programmable --sender A --inputs object(5,0)
//> Root::root::go(Input(0));

// A type-only reference is allowed before and after pause.
//# run Generic::generic::use_type --type-args LeafV1::leaf::Type --sender A

//# programmable --sender A --inputs object(0x426) object(1,1)
// The pause is not visible to this transaction's signing check, so this call executes.
//> 0: sui::package_config::enable_global_pause(Input(0), Input(1));
//> 1: LeafV1::leaf::ping();

// Both versions of the paused package family are rejected.
//# run LeafV1::leaf::ping --sender A

//# run LeafV2::leaf::ping --sender A

// Root's executable transitive dependency is rejected.
//# programmable --sender A --inputs object(5,0)
//> Root::root::go(Input(0));

// Type-only references remain allowed.
//# run Generic::generic::use_type --type-args LeafV1::leaf::Type --sender A

//# programmable --sender A --inputs object(0x426) object(1,1)
// The existing pause is visible to signing, so the PTB is denied before disable executes.
//> 0: sui::package_config::disable_global_pause(Input(0), Input(1));
//> 1: LeafV1::leaf::ping();

// A separate disable transaction restores execution.
//# run sui::package_config::disable_global_pause --args object(0x426) object(1,1) --sender A

//# run LeafV1::leaf::ping --sender A

//# run LeafV2::leaf::ping --sender A

//# programmable --sender A --inputs object(5,0)
//> Root::root::go(Input(0));

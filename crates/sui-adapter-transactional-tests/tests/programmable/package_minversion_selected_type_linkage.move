// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Package selection applies to package addresses found in transaction inputs, not only to the
// function being called. Here a type argument, a MakeMoveVec type, or an object from TypesV1 selects
// TypesV2 and adds TypesV2's LeafV2 dependency to the transaction:
//
//   type or object from TypesV1 -> selected TypesV2 -> LeafV2
//   Root                                      -----> LeafV1
//
// Root's public call allows its Leaf dependency to move forward to LeafV2, so it succeeds and uses
// LeafV2. Root's entry call pins LeafV1 exactly, so combining it with the selected type package
// produces a linkage conflict. The fixture covers each distinct adapter path that discovers package
// addresses in these inputs.

//# init --addresses LeafV1=0x0 LeafV2=0x0 TypesV1=0x0 TypesV2=0x0 Root=0x0 Generic=0x0 --accounts A

//# publish --upgradeable --sender A
module LeafV1::leaf;
public fun value(): u64 { 1 }

//# upgrade --package LeafV1 --upgrade-capability 1,1 --sender A
module LeafV2::leaf;
public fun value(): u64 { 2 }

//# publish --upgradeable --dependencies LeafV1 --sender A
module TypesV1::types;
use LeafV1::leaf;

public struct T has copy, drop {}
public struct O has key, store { id: sui::object::UID }

public fun make(ctx: &mut sui::tx_context::TxContext) {
    assert!(leaf::value() == 1);
    sui::transfer::transfer(O { id: sui::object::new(ctx) }, ctx.sender())
}

//# run TypesV1::types::make --sender A

//# upgrade --package TypesV1 --upgrade-capability 3,1 --dependencies LeafV2 --sender A
module TypesV2::types;
use LeafV2::leaf;

public struct T has copy, drop {}
public struct O has key, store { id: sui::object::UID }

public fun make(ctx: &mut sui::tx_context::TxContext) {
    assert!(leaf::value() == 2);
    sui::transfer::transfer(O { id: sui::object::new(ctx) }, ctx.sender())
}

//# publish --dependencies LeafV1 --sender A
module Root::root;
use LeafV1::leaf;

public struct Result has key, store { id: sui::object::UID, value: u64 }
public fun make(ctx: &mut sui::tx_context::TxContext) {
    sui::transfer::share_object(Result { id: sui::object::new(ctx), value: 0 })
}
public fun public_call(result: &mut Result) { result.value = leaf::value() }
entry fun entry_call(result: &mut Result) { result.value = leaf::value() }

//# run Root::root::make --sender A

//# publish --sender A
module Generic::generic;
public fun use_type<T>() {}

//# programmable --sender A --inputs object(3,1) object(0xcf6)
//> 0: sui::package::enable_minversion_permanently(Input(0));
//> sui::package_config::record_minversion_enrollment(Input(1), Result(0));

// The selected TypesV2 table lifts Root's Leaf dependency to v2.
//# programmable --sender A --inputs object(7,0)
//> 0: Root::root::public_call(Input(0));
//> 1: Generic::generic::use_type<TypesV1::types::T>();

//# view-object 7,0

// An entry call pins LeafV1 exactly and conflicts with the selected type package's LeafV2 table.
//# programmable --sender A --inputs object(7,0)
//> 0: Root::root::entry_call(Input(0));
//> 1: Generic::generic::use_type<TypesV1::types::T>();

// MakeMoveVec follows the same type-package path and lifts the public call to LeafV2.
//# programmable --sender A --inputs object(7,0)
//> 0: Root::root::public_call(Input(0));
//> 1: MakeMoveVec<TypesV1::types::T>([]);

//# view-object 7,0

// A fixed object input from TypesV1 also selects TypesV2 and expands its LeafV2 table.
//# programmable --sender A --inputs object(7,0) object(4,0) @A
//> 0: Root::root::public_call(Input(0));
//> TransferObjects([Input(1)], Input(2));

//# view-object 7,0

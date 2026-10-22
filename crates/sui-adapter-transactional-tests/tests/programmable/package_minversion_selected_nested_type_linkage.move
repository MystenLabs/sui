// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A nested type argument names two enrolled type-package families:
//
//   TypesAV1::Wrap<TypesBV1::Item>
//       | selected TypesAV2 -> LeafAV2
//       ` selected TypesBV2 -> LeafBV2
//
// Combining that type with Root's public call raises both executable leaf dependencies to v2. A
// type-only use remains allowed after LeafAV2 is forbidden, while Root plus the nested selected type
// is rejected because it would execute the forbidden LeafAV2 selected only through type linkage.

//# init --addresses LeafAV1=0x0 LeafAV2=0x0 LeafAV3=0x0 LeafBV1=0x0 LeafBV2=0x0 TypesAV1=0x0 TypesAV2=0x0 TypesBV1=0x0 TypesBV2=0x0 Root=0x0 Generic=0x0 --accounts A

//# publish --upgradeable --sender A
module LeafAV1::leaf_a;
public struct Marker has copy, drop {}
public fun value(): u64 { 1 }

//# upgrade --package LeafAV1 --upgrade-capability 1,1 --sender A
module LeafAV2::leaf_a;
public struct Marker has copy, drop {}
public fun value(): u64 { 2 }

//# upgrade --package LeafAV2 --upgrade-capability 1,1 --sender A
module LeafAV3::leaf_a;
public struct Marker has copy, drop {}
public fun value(): u64 { 3 }

//# publish --upgradeable --sender A
module LeafBV1::leaf_b;
public struct Marker has copy, drop {}
public fun value(): u64 { 1 }

//# upgrade --package LeafBV1 --upgrade-capability 4,1 --sender A
module LeafBV2::leaf_b;
public struct Marker has copy, drop {}
public fun value(): u64 { 2 }

//# publish --upgradeable --dependencies LeafAV1 --sender A
module TypesAV1::types_a;
use LeafAV1::leaf_a;
public struct Wrap<T: copy + drop> has copy, drop { marker: leaf_a::Marker, value: T }

//# upgrade --package TypesAV1 --upgrade-capability 6,1 --dependencies LeafAV2 --sender A
module TypesAV2::types_a;
use LeafAV2::leaf_a;
public struct Wrap<T: copy + drop> has copy, drop { marker: leaf_a::Marker, value: T }

//# publish --upgradeable --dependencies LeafBV1 --sender A
module TypesBV1::types_b;
use LeafBV1::leaf_b;
public struct Item has copy, drop { marker: leaf_b::Marker }

//# upgrade --package TypesBV1 --upgrade-capability 8,1 --dependencies LeafBV2 --sender A
module TypesBV2::types_b;
use LeafBV2::leaf_b;
public struct Item has copy, drop { marker: leaf_b::Marker }

//# publish --dependencies LeafAV1 LeafBV1 --sender A
module Root::root;
use LeafAV1::leaf_a;
use LeafBV1::leaf_b;
public struct Result has key, store { id: sui::object::UID, a: u64, b: u64 }
public fun make(ctx: &mut sui::tx_context::TxContext) {
    sui::transfer::public_transfer(Result { id: sui::object::new(ctx), a: 0, b: 0 }, ctx.sender())
}
public fun call(result: &mut Result) {
    result.a = leaf_a::value();
    result.b = leaf_b::value();
}

//# run Root::root::make --sender A

//# publish --sender A
module Generic::generic;
public fun use_type<T>() {}

//# programmable --sender A --inputs object(6,1) object(8,1) object(0xcf6)
//> 0: sui::package::enable_minversion_permanently(Input(0));
//> 1: sui::package_config::record_minversion_enrollment(Input(2), Result(0));
//> 2: sui::package::enable_minversion_permanently(Input(1));
//> sui::package_config::record_minversion_enrollment(Input(2), Result(2));

// Both selected type packages raise Root's executable leaves to v2.
//# programmable --sender A --inputs object(11,0)
//> 0: Root::root::call(Input(0));
//> 1: Generic::generic::use_type<TypesAV1::types_a::Wrap<TypesBV1::types_b::Item>>();

//# view-object 11,0

// LeafAV2 is historical because LeafAV3 exists.
//# run sui::package_config::forbid_version --args object(0xcf6) object(1,1) 2 --sender A

// Type-only use does not execute LeafA code and remains allowed.
//# run Generic::generic::use_type --type-args TypesAV1::types_a::Wrap<TypesBV1::types_b::Item> --sender A

// Root alone still executes its allowed historical v1 leaves.
//# programmable --sender A --inputs object(11,0)
//> Root::root::call(Input(0));

//# view-object 11,0

// The selected nested type raises Root's LeafA dependency to forbidden v2, so this is rejected.
//# programmable --sender A --inputs object(11,0)
//> 0: Root::root::call(Input(0));
//> 1: Generic::generic::use_type<TypesAV1::types_a::Wrap<TypesBV1::types_b::Item>>();

//# view-object 11,0

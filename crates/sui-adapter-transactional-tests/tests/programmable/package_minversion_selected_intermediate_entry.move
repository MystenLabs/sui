// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// An entry call requires the exact dependency versions recorded when its package was published.
// ConsumerS therefore requires MidSV1 and LeafSV3. Enrolling the Mid family redirects MidSV1 to
// MidSV2, and the call must use that exact selected Mid version. MidSV2's own LeafSV2 dependency is
// different: it is a minimum version requirement, so a newer Leaf version may satisfy it. This lets
// ConsumerS keep its exact LeafSV3 while running the selected MidSV2.
//
// Success case:
//
//   ConsumerS entry closure       selection and expansion       unified linkage
//   -----------------------       -----------------------       ---------------
//   Exact(MidSV1)          -----> Exact(MidSV2)                 Exact(MidSV2)
//   Exact(LeafSV3)                MidSV2 -> AtLeast(LeafSV2)    Exact(LeafSV3)
//
// `Exact(LeafSV3)` satisfies `AtLeast(LeafSV2)`, so the selected MidSV2 executes with LeafSV3.
//
// Failure case:
//
//   ConsumerF entry closure       selection and expansion
//   -----------------------       -----------------------
//   Exact(MidFV1)          -----> Exact(MidFV2)
//   Exact(LeafFV2)                MidFV2 -> AtLeast(LeafFV3)
//
// `Exact(LeafFV2)` cannot satisfy `AtLeast(LeafFV3)`, so linkage resolution fails.

//# init --addresses LeafSV1=0x0 LeafSV2=0x0 LeafSV3=0x0 MidSV1=0x0 MidSV2=0x0 ConsumerS=0x0 LeafFV1=0x0 LeafFV2=0x0 LeafFV3=0x0 MidFV1=0x0 MidFV2=0x0 ConsumerF=0x0 --accounts A

//# publish --upgradeable --sender A
module LeafSV1::leaf;
public fun value(): u64 { 1 }

//# upgrade --package LeafSV1 --upgrade-capability 1,1 --sender A
module LeafSV2::leaf;
public fun value(): u64 { 2 }

//# upgrade --package LeafSV2 --upgrade-capability 1,1 --sender A
module LeafSV3::leaf;
public fun value(): u64 { 3 }

//# publish --upgradeable --dependencies LeafSV1 --sender A
module MidSV1::mid;
use LeafSV1::leaf;
public struct Selected has copy, drop { version: u64 }
public fun value(): u64 {
    sui::event::emit(Selected { version: 1 });
    leaf::value()
}

//# upgrade --package MidSV1 --upgrade-capability 4,1 --dependencies LeafSV2 --sender A
module MidSV2::mid;
use LeafSV2::leaf;
public struct Selected has copy, drop { version: u64 }
public fun value(): u64 {
    sui::event::emit(Selected { version: 2 });
    leaf::value()
}

//# publish --dependencies MidSV1 LeafSV3 --sender A
module ConsumerS::consumer;
use MidSV1::mid;
public struct Leaf has copy, drop { version: u64 }
entry fun call() { sui::event::emit(Leaf { version: mid::value() }) }

//# programmable --sender A --inputs object(4,1) object(0xcf6)
//> 0: sui::package::enable_minversion_permanently(Input(0));
//> sui::package_config::record_minversion_enrollment(Input(1), Result(0));

// Entry linkage pins the consumer's LeafSV3 exactly. Selecting MidSV2 expands its older LeafSV2
// edge as at-least, so LeafSV3 satisfies it and both emitted versions prove the final graph.
//# run ConsumerS::consumer::call --sender A

// Enrolling the leaf too exercises nested selection through the selected intermediate table.
//# programmable --sender A --inputs object(1,1) object(0xcf6)
//> 0: sui::package::enable_minversion_permanently(Input(0));
//> sui::package_config::record_minversion_enrollment(Input(1), Result(0));

//# run ConsumerS::consumer::call --sender A

//# publish --upgradeable --sender A
module LeafFV1::leaf;
public fun value(): u64 { 1 }

//# upgrade --package LeafFV1 --upgrade-capability 11,1 --sender A
module LeafFV2::leaf;
public fun value(): u64 { 2 }

//# upgrade --package LeafFV2 --upgrade-capability 11,1 --sender A
module LeafFV3::leaf;
public fun value(): u64 { 3 }

//# publish --upgradeable --dependencies LeafFV1 --sender A
module MidFV1::mid;
use LeafFV1::leaf;
public fun value(): u64 { leaf::value() }

//# upgrade --package MidFV1 --upgrade-capability 14,1 --dependencies LeafFV3 --sender A
module MidFV2::mid;
use LeafFV3::leaf;
public fun value(): u64 { leaf::value() }

//# publish --dependencies MidFV1 LeafFV2 --sender A
module ConsumerF::consumer;
use MidFV1::mid;
public struct Leaf has copy, drop { version: u64 }
entry fun call() { sui::event::emit(Leaf { version: mid::value() }) }

//# programmable --sender A --inputs object(14,1) object(0xcf6)
//> 0: sui::package::enable_minversion_permanently(Input(0));
//> sui::package_config::record_minversion_enrollment(Input(1), Result(0));

// The entry closure pins LeafFV2 exactly, which cannot satisfy selected MidFV2's LeafFV3 edge.
//# run ConsumerF::consumer::call --sender A

// Explicitly enrolling Leaf selects LeafFV3 for ConsumerF's historical LeafFV2 reference.
//# programmable --sender A --inputs object(11,1) object(0xcf6)
//> 0: sui::package::enable_minversion_permanently(Input(0));
//> sui::package_config::record_minversion_enrollment(Input(1), Result(0));

// The selected LeafFV3 now satisfies selected MidFV2's minimum LeafFV3 dependency.
//# run ConsumerF::consumer::call --sender A

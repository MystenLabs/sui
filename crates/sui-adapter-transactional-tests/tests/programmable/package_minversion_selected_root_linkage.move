// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// RootV1 was published with LeafV1, while RootV2 was published with LeafV2. After enrolling the
// Root family at v2, a call that still names RootV1 must be resolved as:
//
//   requested: RootV1 -> LeafV1
//   selected:  RootV2 -> LeafV2
//
// Linkage analysis must use RootV2's dependency table, not the stale table from RootV1. The public
// and entry calls both emit the leaf version so the selected graph is observable.

//# init --addresses LeafV1=0x0 LeafV2=0x0 RootV1=0x0 RootV2=0x0 --accounts A

//# publish --upgradeable --sender A
module LeafV1::leaf;
public fun value(): u64 { 1 }

//# upgrade --package LeafV1 --upgrade-capability 1,1 --sender A
module LeafV2::leaf;
public fun value(): u64 { 2 }

//# publish --upgradeable --dependencies LeafV1 --sender A
module RootV1::root;
use LeafV1::leaf;

public struct Ping has copy, drop { version: u64 }
public fun public_call() { sui::event::emit(Ping { version: leaf::value() }) }
entry fun entry_call() { sui::event::emit(Ping { version: leaf::value() }) }

//# upgrade --package RootV1 --upgrade-capability 3,1 --dependencies LeafV2 --sender A
module RootV2::root;
use LeafV2::leaf;

public struct Ping has copy, drop { version: u64 }
public fun public_call() { sui::event::emit(Ping { version: leaf::value() }) }
entry fun entry_call() { sui::event::emit(Ping { version: leaf::value() }) }

//# programmable --sender A --inputs object(3,1) object(0xcf6)
//> 0: sui::package::enable_minversion_permanently(Input(0));
//> sui::package_config::record_minversion_enrollment(Input(1), Result(0));

// RootV1 is redirected before visibility-based linkage analysis, so both calls use RootV2's
// LeafV2 dependency table and emit version 2.
//# run RootV1::root::public_call --sender A

// The historical entry call is redirected in the same way.
//# run RootV1::root::entry_call --sender A

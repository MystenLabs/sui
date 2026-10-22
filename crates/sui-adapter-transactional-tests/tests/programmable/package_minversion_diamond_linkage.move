// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Two selected intermediate packages form a diamond over one leaf family:
//
//   Consumer -> selected LeftV2  -> LeafV2
//            -> selected RightV2 -> LeafV3
//
// A public consumer permits upward dependency unification, so both branches execute LeafV3. An
// entry consumer retains its exact historical LeafV1 constraint and conflicts with the selected
// branches' newer leaf requirements.

//# init --addresses LeafV1=0x0 LeafV2=0x0 LeafV3=0x0 LeftV1=0x0 LeftV2=0x0 RightV1=0x0 RightV2=0x0 Consumer=0x0 --accounts A

//# publish --upgradeable --sender A
module LeafV1::leaf;
public fun value(): u64 { 1 }

//# upgrade --package LeafV1 --upgrade-capability 1,1 --sender A
module LeafV2::leaf;
public fun value(): u64 { 2 }

//# upgrade --package LeafV2 --upgrade-capability 1,1 --sender A
module LeafV3::leaf;
public fun value(): u64 { 3 }

//# publish --upgradeable --dependencies LeafV1 --sender A
module LeftV1::left;
use LeafV1::leaf;
public fun value(): u64 { leaf::value() }

//# upgrade --package LeftV1 --upgrade-capability 4,1 --dependencies LeafV2 --sender A
module LeftV2::left;
use LeafV2::leaf;
public fun value(): u64 { leaf::value() }

//# publish --upgradeable --dependencies LeafV1 --sender A
module RightV1::right;
use LeafV1::leaf;
public fun value(): u64 { leaf::value() }

//# upgrade --package RightV1 --upgrade-capability 6,1 --dependencies LeafV3 --sender A
module RightV2::right;
use LeafV3::leaf;
public fun value(): u64 { leaf::value() }

//# publish --dependencies LeftV1 RightV1 LeafV1 --sender A
module Consumer::consumer;
use LeftV1::left;
use RightV1::right;
public struct Versions has copy, drop { left: u64, right: u64 }
public fun public_call() {
    sui::event::emit(Versions { left: left::value(), right: right::value() })
}
entry fun entry_call() {
    sui::event::emit(Versions { left: left::value(), right: right::value() })
}

//# programmable --sender A --inputs object(4,1) object(6,1) object(0xcf6)
//> 0: sui::package::enable_minversion_permanently(Input(0));
//> 1: sui::package_config::record_minversion_enrollment(Input(2), Result(0));
//> 2: sui::package::enable_minversion_permanently(Input(1));
//> sui::package_config::record_minversion_enrollment(Input(2), Result(2));

// Public linkage raises both diamond branches to LeafV3.
//# run Consumer::consumer::public_call --sender A

// Entry linkage pins the historical LeafV1 exactly and cannot satisfy either selected branch.
//# run Consumer::consumer::entry_call --sender A

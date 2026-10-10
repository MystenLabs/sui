// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Consumer was published against BaseV1, which has no dependencies. Selecting BaseV2 changes the
// graph by adding NewV1:
//
//   published graph:   Consumer -> BaseV1
//   selected graph:    Consumer -> BaseV2 -> NewV1
//
// Linkage analysis must load and validate this newly introduced package. The first call proves that
// NewV1 executes; the second proves that the same call is rejected after NewV1 is forbidden.

//# init --addresses NewV1=0x0 NewV2=0x0 BaseV1=0x0 BaseV2=0x0 Consumer=0x0 --accounts A

//# publish --upgradeable --sender A
module NewV1::new_dependency;
public struct Ping has copy, drop { version: u64 }
public fun call() { sui::event::emit(Ping { version: 1 }) }

//# publish --upgradeable --sender A
module BaseV1::base;
public fun call() {}

//# upgrade --package BaseV1 --upgrade-capability 2,1 --dependencies NewV1 --sender A
module BaseV2::base;
use NewV1::new_dependency;
public fun call() { new_dependency::call() }

//# publish --dependencies BaseV1 --sender A
module Consumer::consumer;
use BaseV1::base;
public fun call() { base::call() }

//# programmable --sender A --inputs object(2,1) object(0xcf6)
//> 0: sui::package::enable_minversion_permanently(Input(0));
//> sui::package_config::record_minversion_enrollment(Input(1), Result(0));

// BaseV1 selects BaseV2, whose newly introduced NewV1 dependency executes observably.
//# run Consumer::consumer::call --sender A

// Make NewV1 historical, then forbid the exact version introduced by BaseV2's selected table.
//# upgrade --package NewV1 --upgrade-capability 1,1 --sender A
module NewV2::new_dependency;
public struct Ping has copy, drop { version: u64 }
public fun call() { sui::event::emit(Ping { version: 2 }) }

//# run sui::package_config::forbid_version --args object(0xcf6) object(1,1) 1 --sender A

//# run Consumer::consumer::call --sender A

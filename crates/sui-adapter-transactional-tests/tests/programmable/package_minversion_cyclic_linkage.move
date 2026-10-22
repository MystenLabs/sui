// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Each package upgrade below is valid by itself:
//
//   AV2 -> BV1        BV2 -> AV1
//
// Once the B family selects BV2, a call to AV2 resolves the package families to:
//
//   AV2 -> BV2 -> AV2
//
// The selected versions now form a cycle even though neither upgrade introduced a cycle when it
// was published. Linkage analysis must detect the final cycle before running either a public or an
// entry function.

//# init --addresses AV1=0x0 AV2=0x0 BV1=0x0 BV2=0x0 --accounts A

//# publish --upgradeable --sender A
module AV1::a;
public fun public_call() {}
entry fun entry_call() {}

//# publish --upgradeable --sender A
module BV1::b;
public fun ping() {}

//# programmable --sender A --inputs object(2,1) object(0xcf6)
//> 0: sui::package::enable_minversion_permanently(Input(0));
//> sui::package_config::record_minversion_enrollment(Input(1), Result(0));

// AV2 -> BV1 is acyclic when this version is published.
//# upgrade --package AV1 --upgrade-capability 1,1 --dependencies BV1 --sender A
module AV2::a;
use BV1::b;
public fun public_call() { b::ping() }
entry fun entry_call() { b::ping() }

// BV2 -> AV1 is also acyclic when this version is published. Selecting BV2 for AV2's historical
// BV1 edge creates the effective cycle AV2 -> BV2 -> AV2.
//# upgrade --package BV1 --upgrade-capability 2,1 --dependencies AV1 --sender A --minversion
module BV2::b;
use AV1::a;
public fun ping() { a::public_call() }

//# run AV2::a::public_call --sender A

// Exact dependency constraints from an entry call reject the same effective cycle.
//# run AV2::a::entry_call --sender A

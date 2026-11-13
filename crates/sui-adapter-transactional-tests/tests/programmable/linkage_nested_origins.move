// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Type names spelled through v3 contribute their defining versions: v1 for the outer type
// and v2 for a nested type introduced in v2. Tests nested MoveCall and MakeMoveVec types,
// version-specific conflicts, and deduplication/order independence when types defined in
// different versions of the same package occur together.

//# init --addresses DepV1=0x0 DepV2=0x0 DepV3=0x0 Generic=0x0 --accounts A

//# publish --upgradeable --sender A
module DepV1::m {
    public struct Old has drop {}
    public struct Outer<phantom T> has drop {}

    public fun check(expected: u64) { assert!(expected == 1, 0) }
}

//# upgrade --package DepV1 --upgrade-capability 1,1 --sender A
module DepV2::m {
    public struct Old has drop {}
    public struct Outer<phantom T> has drop {}
    public struct New has drop {}

    public fun check(expected: u64) { assert!(expected == 2, 0) }
}

//# upgrade --package DepV2 --upgrade-capability 1,1 --sender A
module DepV3::m {
    public struct Old has drop {}
    public struct Outer<phantom T> has drop {}
    public struct New has drop {}
    public struct Newest has drop {}

    public fun check(expected: u64) { assert!(expected == 3, 0) }
}

//# publish --sender A
module Generic::m {
    public fun generic<T>() {}
}

//# programmable --sender A --inputs 2u64
// The outer type is defined in v1, the inner type in v2: neither v3 spelling requires v3.
//> Generic::m::generic<DepV3::m::Outer<vector<DepV3::m::New>>>();
//> MakeMoveVec<DepV3::m::Outer<vector<DepV3::m::New>>>([]);
//> DepV2::m::check(Input(0));

//# programmable --sender A --inputs 1u64
// The nested v2 defining ID still conflicts with an exact v1 target.
//> Generic::m::generic<DepV3::m::Outer<vector<DepV3::m::New>>>();
//> DepV1::m::check(Input(0));

//# programmable --sender A --inputs 2u64
// Expanding the v1 defining ID first must not deduplicate away the later v2 requirement.
//> Generic::m::generic<DepV1::m::Old>();
//> Generic::m::generic<DepV3::m::New>();
//> Generic::m::generic<DepV3::m::Old>();
//> DepV2::m::check(Input(0));

//# programmable --sender A --inputs 2u64
// Reversing the type contributions still selects v2, not v1 or v3.
//> Generic::m::generic<DepV3::m::New>();
//> Generic::m::generic<DepV1::m::Old>();
//> DepV2::m::check(Input(0));

//# programmable --sender A --inputs 2u64
// A genuinely v3-defined type cannot be loaded under an exact v2 selection.
//> Generic::m::generic<DepV3::m::Outer<DepV3::m::Newest>>();
//> DepV2::m::check(Input(0));

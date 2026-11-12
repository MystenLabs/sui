// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Preparation normalizes type names to defining IDs and collects linkage before VM loading
// or typing. Tests old versus newly introduced types, later linkage conflicts preceding
// earlier arity errors, and metadata/name-resolution failures preceding signature validation.

//# init --addresses DepV1=0x0 DepV2=0x0 Consumer=0x0 --accounts A

//# publish --upgradeable --sender A
module DepV1::m {
    public struct Old has drop {}

    public fun ping() {}
}

//# publish --dependencies DepV1 --sender A
module Consumer::m {
    public fun generic<T>() {}

    public fun argument(_: u64) {}
}

//# upgrade --package DepV1 --upgrade-capability 1,1 --sender A
module DepV2::m {
    public struct Old has drop {}
    public struct New has drop {}

    public fun ping() {}
}

//# programmable --sender A
// Naming an old type through v2 only contributes its v1 defining ID, including nested types.
//> Consumer::m::generic<vector<DepV2::m::Old>>();
//> MakeMoveVec<DepV2::m::Old>([]);
//> DepV1::m::ping();

//# programmable --sender A
// A genuinely new type still requires v2 and conflicts with the exact v1 target.
//> Consumer::m::generic<DepV2::m::New>();
//> DepV1::m::ping();

//# programmable --sender A
// Without a linkage conflict, loading reports the invalid number of function type arguments.
//> Consumer::m::generic();

//# programmable --sender A
// A later linkage conflict wins over the earlier function type-arity error.
//> Consumer::m::generic();
//> DepV1::m::ping();
//> DepV2::m::ping();

//# programmable --sender A
// Without a linkage conflict, typing reports the missing value argument.
//> Consumer::m::argument();

//# programmable --sender A
// A later linkage conflict also wins over the earlier value-argument error.
//> Consumer::m::argument();
//> DepV1::m::ping();
//> DepV2::m::ping();

//# programmable --sender A
// Missing functions fail during preparation, before instantiated signature validation.
//> Consumer::m::generic();
//> DepV1::m::missing();

//# programmable --sender A
// Unresolved type names also fail during preparation.
//> Consumer::m::generic();
//> Consumer::m::generic<DepV2::m::Missing>();

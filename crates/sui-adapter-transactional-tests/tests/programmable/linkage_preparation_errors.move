// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Preparation-time name lookup errors retain command and outer type-argument indices,
// including nested types at argument 1. Generic arity and ability checks are deferred to
// loading: later linkage conflicts precede them, while earlier ordinary preparation failures
// precede later conflicts.

//# init --addresses DepV1=0x0 DepV2=0x0 Generic=0x0 --accounts A

//# publish --upgradeable --sender A
module DepV1::m;
public struct Box<T: copy + drop> has drop { value: T }
public struct NoCopy has drop {}
public fun ping() {}

//# upgrade --package DepV1 --upgrade-capability 1,1 --sender A
module DepV2::m;
public struct Box<T: copy + drop> has drop { value: T }
public struct NoCopy has drop {}
public fun ping() {}

//# publish --sender A
module Generic::m;
public fun pair<T, U>() {}

//# programmable --sender A
// Valid nested instantiation establishes the positive control.
//> Generic::m::pair<u64, vector<DepV1::m::Box<u64>>>();

//# programmable --sender A
// Name resolution must retain the outer argument index 1 and command index 1.
//> DepV1::m::ping();
//> Generic::m::pair<u64, vector<DepV1::m::Missing>>();

//# programmable --sender A
// Existing type names normalize successfully; generic arity is rejected during loading.
//> DepV1::m::ping();
//> Generic::m::pair<u64, DepV1::m::Box<u64, u64>>();

//# programmable --sender A
// A nested ability violation is also deferred, with argument index 1 and command index 1.
//> DepV1::m::ping();
//> Generic::m::pair<u64, vector<DepV1::m::Box<DepV1::m::NoCopy>>>();

//# programmable --sender A
// The later ordinary linkage conflict precedes deferred instantiation validation.
//> Generic::m::pair<u64, DepV1::m::Box<DepV1::m::NoCopy>>();
//> DepV1::m::ping();
//> DepV2::m::ping();

//# programmable --sender A
// An unresolved name is an ordinary preparation failure and precedes the later conflict.
//> Generic::m::pair<u64, vector<DepV1::m::Missing>>();
//> DepV1::m::ping();
//> DepV2::m::ping();

//# programmable --sender A
// The first ordinary metadata failure wins over a later exact/exact conflict.
//> DepV1::m::missing();
//> DepV1::m::ping();
//> DepV2::m::ping();

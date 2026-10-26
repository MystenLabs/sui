// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Package-version rules activate in protocol version 141. At v140 there is no PackageConfig
// object, so a historical package reference must execute exactly the version it names without an
// implicit PackageConfig read.

//# init --addresses BaseV1=0x0 BaseV2=0x0 --accounts A --protocol-version 140

//# publish --upgradeable --sender A
module BaseV1::base;
public struct Ping has copy, drop { version: u64 }
public fun ping() { sui::event::emit(Ping { version: 1 }) }

//# upgrade --package BaseV1 --upgrade-capability 1,1 --sender A
module BaseV2::base;
public struct Ping has copy, drop { version: u64 }
public fun ping() { sui::event::emit(Ping { version: 2 }) }

// v140 executes the explicitly referenced historical package. The snapshot must contain the v1
// event and no unchanged 0xcf6 shared input.
//# run BaseV1::base::ping --sender A

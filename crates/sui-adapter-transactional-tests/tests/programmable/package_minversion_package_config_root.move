// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// The package-config object is an implicit read-only root whenever execution uses minversion.
// Missing settings still require a root snapshot; an explicit input is not additionally tracked
// as an implicit read.

//# init --addresses BaseV1=0x0 BaseV2=0x0 --accounts A

//# publish --upgradeable --sender A
module BaseV1::base;

public struct Ping has copy, drop { version: u64 }
public fun ping() { sui::event::emit(Ping { version: 1 }) }

//# upgrade --package BaseV1 --upgrade-capability 1,1 --sender A
module BaseV2::base;

public struct Ping has copy, drop { version: u64 }
public fun ping() { sui::event::emit(Ping { version: 2 }) }

// There is no setting. This still records the implicit package-config root read.
//# run BaseV2::base::ping --sender A

//# programmable --sender A --inputs object(1,1) object(0x426)
//> 0: sui::package::enable_minversion(Input(0));
//> sui::package_config::record_minversion_enrollment(Input(1), Result(0));

// The setting is immediately selected. Historical BaseV1 emits version 2 from BaseV2 and records
// package config as an unchanged consensus read.
//# run BaseV1::base::ping --sender A

// Supplying package config explicitly still resolves historical BaseV1 to BaseV2, but it is an
// input rather than an additional implicit root read.
//# programmable --sender A --inputs object(0x426)
//> BaseV1::base::ping();

// The same resolution holds for a non-mutable shared input: it is a read-only-root effect, not
// an additional implicit root read.
//# programmable --sender A --inputs immshared(0x426)
//> BaseV1::base::ping();

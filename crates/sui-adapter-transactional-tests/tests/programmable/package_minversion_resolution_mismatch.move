// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Upgrade resolution must match the cap's minversion state. Both failures are atomic.

//# init --addresses EnrolledV1=0x0 EnrolledV2=0x0 OrdinaryV1=0x0 OrdinaryV2=0x0 --accounts A

//# publish --upgradeable --sender A
module EnrolledV1::enrolled;
public struct Ping has copy, drop { version: u64 }
public fun ping() { sui::event::emit(Ping { version: 1 }) }

//# programmable --sender A --inputs object(1,1) object(0x426)
//> 0: sui::package::enable_minversion(Input(0));
//> 1: sui::package_config::record_minversion_enrollment(Input(1), Result(0));

// An enrolled cap rejects an ordinary upgrade emitted without --minversion.
//# upgrade --package EnrolledV1 --upgrade-capability 1,1 --sender A
module EnrolledV2::enrolled;
public struct Ping has copy, drop { version: u64 }
public fun ping() { sui::event::emit(Ping { version: 2 }) }

// The failed upgrade is atomic: the historical v1 call still emits version 1.
//# run EnrolledV1::enrolled::ping --sender A

//# publish --upgradeable --sender A
module OrdinaryV1::ordinary;
public struct Ping has copy, drop { version: u64 }
public fun ping() { sui::event::emit(Ping { version: 1 }) }

// An unenrolled cap rejects a minversion upgrade emitted by --minversion.
//# upgrade --package OrdinaryV1 --upgrade-capability 5,1 --sender A --minversion
module OrdinaryV2::ordinary;
public struct Ping has copy, drop { version: u64 }
public fun ping() { sui::event::emit(Ping { version: 2 }) }

// Both failed upgrades are atomic. The enrolled cap can still upgrade through the minversion
// path, and its historical v1 reference then emits version 2 from the upgraded implementation.
//# upgrade --package EnrolledV1 --upgrade-capability 1,1 --sender A --minversion
module EnrolledV2::enrolled;
public struct Ping has copy, drop { version: u64 }
public fun ping() { sui::event::emit(Ping { version: 2 }) }

//# run EnrolledV1::enrolled::ping --sender A

// The ordinary cap likewise still upgrades through the ordinary path.
//# upgrade --package OrdinaryV1 --upgrade-capability 5,1 --sender A
module OrdinaryV2::ordinary;
public struct Ping has copy, drop { version: u64 }
public fun ping() { sui::event::emit(Ping { version: 2 }) }

// The direct new-version call emits version 2.
//# run OrdinaryV2::ordinary::ping --sender A

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Calling BaseV1 after enrolling its package family should run BaseV2. This must work the same way
// in committed execution, dry-run, and dev-inspect: all three modes must read the same PackageConfig
// state and report the event emitted by BaseV2.
//
// The runner defaults to the current maximum protocol version (141), where selection is enabled.
//# init --addresses BaseV1=0x0 BaseV2=0x0 --accounts A

//# publish --upgradeable --sender A
module BaseV1::base;
public struct Ping has copy, drop { version: u64 }
public fun ping() { sui::event::emit(Ping { version: 1 }) }

//# upgrade --package BaseV1 --upgrade-capability 1,1 --sender A
module BaseV2::base;
public struct Ping has copy, drop { version: u64 }
public fun ping() { sui::event::emit(Ping { version: 2 }) }

//# programmable --sender A --inputs object(1,1) object(0xcf6)
//> 0: sui::package::enable_minversion_permanently(Input(0));
//> sui::package_config::record_minversion_enrollment(Input(1), Result(0));

// Committed execution selects BaseV2.
//# programmable --sender A
//> BaseV1::base::ping();

// Dry-run selects BaseV2 without committing effects.
//# programmable --sender A --dry-run
//> BaseV1::base::ping();

// Dev-inspect also selects BaseV2.
//# programmable --sender A --dev-inspect
//> BaseV1::base::ping();

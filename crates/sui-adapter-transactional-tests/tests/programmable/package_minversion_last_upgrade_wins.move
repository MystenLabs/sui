// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Multiple enrolled upgrades leave only the final package as the selected version. The newest
// explicit package remains usable throughout.

//# init --addresses BaseV1=0x0 BaseV2=0x0 BaseV3=0x0 --accounts A

//# publish --upgradeable --sender A
module BaseV1::base;
public fun ping() { abort 1 }

//# programmable --sender A --inputs object(1,1) object(0xcf6)
//> 0: sui::package::enable_minversion_permanently(Input(0));
//> 1: sui::package_config::record_minversion_enrollment(Input(1), Result(0));

//# upgrade --package BaseV1 --upgrade-capability 1,1 --sender A --minversion
module BaseV2::base;
public fun ping() { abort 2 }

//# upgrade --package BaseV2 --upgrade-capability 1,1 --sender A --minversion
module BaseV3::base;
public struct Ping has copy, drop { version: u64 }
public fun ping() { sui::event::emit(Ping { version: 3 }) }

// v3 is newer than the current selection and must not be downgraded.
//# run BaseV3::base::ping --sender A

// The last upgrade, v3, is selected for historical v1.
//# run BaseV1::base::ping --sender A

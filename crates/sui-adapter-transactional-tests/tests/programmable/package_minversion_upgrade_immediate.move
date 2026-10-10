// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A minversion upgrade must not downgrade an explicit reference to the new package. Older
// references select the new package in the following transaction.

//# init --addresses BaseV1=0x0 BaseV2=0x0 --accounts A

//# publish --upgradeable --sender A
module BaseV1::base;
public fun ping() { abort 1 }

//# programmable --sender A --inputs object(1,1) object(0xcf6)
//> 0: sui::package::enable_minversion_permanently(Input(0));
//> 1: sui::package_config::record_minversion_enrollment(Input(1), Result(0));

//# upgrade --package BaseV1 --upgrade-capability 1,1 --sender A --minversion
module BaseV2::base;
public struct Ping has copy, drop { version: u64 }
public fun ping() { sui::event::emit(Ping { version: 2 }) }

// The v2 reference must execute v2, not be redirected back to stable v1.
//# run BaseV2::base::ping --sender A

// An older v1 reference resolves to v2 immediately after the upgrade.
//# run BaseV1::base::ping --sender A

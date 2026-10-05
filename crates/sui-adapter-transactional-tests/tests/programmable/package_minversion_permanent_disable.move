// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Permanent disable prevents enrollment but preserves ordinary upgrade behavior.

//# init --addresses BaseV1=0x0 BaseV2=0x0 --accounts A

//# publish --upgradeable --sender A
module BaseV1::base;
public fun ping() { abort 1 }

//# run sui::package::disable_minversion_permanently --args object(1,1) --sender A

// A permanently-disabled cap cannot produce an enrollment token.
//# programmable --sender A --inputs object(1,1) object(0x426)
//> 0: sui::package::enable_minversion(Input(0));
//> 1: sui::package_config::record_minversion_enrollment(Input(1), Result(0));

// Ordinary upgrades remain valid for permanently-disabled caps.
//# upgrade --package BaseV1 --upgrade-capability 1,1 --sender A
module BaseV2::base;
public fun ping() {}

//# run BaseV2::base::ping --sender A

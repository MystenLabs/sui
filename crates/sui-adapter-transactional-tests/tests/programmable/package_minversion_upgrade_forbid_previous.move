// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Recording an enrolled upgrade atomically forbids its replaced version and updates minversion
// selection for the next transaction.

//# init --addresses BaseV1=0x0 BaseV2=0x0 BaseV3=0x0 --accounts A

//# publish --upgradeable --sender A
module BaseV1::base;
public fun ping() { abort 1 }

//# programmable --sender A --inputs object(1,1) object(0xcf6)
//> 0: sui::package::enable_minversion_permanently(Input(0));
//> 1: sui::package_config::record_minversion_enrollment(Input(1), Result(0));

//# stage-package
module BaseV2::base;
public struct Ping has copy, drop { version: u64 }
public fun ping() { sui::event::emit(Ping { version: 2 }) }

//# programmable --sender A --inputs object(1,1) object(0xcf6) 0u8 digest(BaseV2)
//> 0: sui::package::authorize_upgrade(Input(0), Input(2), Input(3));
//> 1: Upgrade(BaseV2, [sui,std], BaseV1, Result(0));
//> 2: sui::package_config::record_minversion_upgrade_and_forbid_previous_versions(Input(1), Input(0), Result(1));
//> sui::package::commit_upgrade(Input(0), Result(1));

// The first upgrade selects v2 and records v1 in both the bitmap and watermark.
//# view-object 2,0

//# view-object 4,1

//# view-object 4,2

// v1 immediately resolves to v2, so the v1 forbid no longer applies.
//# run BaseV1::base::ping --sender A

//# set-address BaseV2 object(4,0)

// The newly upgraded package remains directly usable.
//# run BaseV2::base::ping --sender A

//# stage-package
module BaseV3::base;
public struct Ping has copy, drop { version: u64 }
public fun ping() { sui::event::emit(Ping { version: 3 }) }

// The watermark means this upgrade only needs to add the newly historical v2 forbid entry.
//# programmable --sender A --inputs object(1,1) object(0xcf6) 0u8 digest(BaseV3)
//> 0: sui::package::authorize_upgrade(Input(0), Input(2), Input(3));
//> 1: Upgrade(BaseV3, [sui,std], BaseV2, Result(0));
//> 2: sui::package_config::record_minversion_upgrade_and_forbid_previous_versions(Input(1), Input(0), Result(1));
//> sui::package::commit_upgrade(Input(0), Result(1));

// The second upgrade selects v3 and extends both forbidden-version records through v2.
//# view-object 2,0

//# view-object 4,1

//# view-object 4,2

// The historical v1 reference now resolves to v3.
//# run BaseV1::base::ping --sender A

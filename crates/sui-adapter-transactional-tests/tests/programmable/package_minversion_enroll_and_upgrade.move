// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Enrollment and a minversion upgrade can occur in one PTB. The recorded enrollment establishes
// the v1 selection before the normal authorize/Upgrade/record/commit sequence advances it to v2.

//# init --addresses BaseV1=0x0 BaseV2=0x0 --accounts A

//# publish --upgradeable --sender A
module BaseV1::base;
public fun ping() { abort 1 }

//# stage-package
module BaseV2::base;
public struct Ping has copy, drop { version: u64 }
public fun ping() { sui::event::emit(Ping { version: 2 }) }

// Enrollment cannot begin after authorization has cleared the cap's package ID. The failed PTB
// rolls back authorization, so the following PTB can enroll and upgrade successfully.
//# programmable --sender A --inputs object(1,1) object(0xcf6) 0u8 digest(BaseV2)
//> 0: sui::package::authorize_upgrade(Input(0), Input(2), Input(3));
//> 1: Upgrade(BaseV2, [sui,std], BaseV1, Result(0));
//> 2: sui::package::enable_minversion_permanently(Input(0));
//> 3: sui::package_config::record_minversion_enrollment(Input(1), Result(2));
//> sui::package::commit_upgrade(Input(0), Result(1));

//# programmable --sender A --inputs object(1,1) object(0xcf6) 0u8 digest(BaseV2)
//> 0: sui::package::enable_minversion_permanently(Input(0));
//> 1: sui::package_config::record_minversion_enrollment(Input(1), Result(0));
//> 2: sui::package::authorize_upgrade(Input(0), Input(2), Input(3));
//> 3: Upgrade(BaseV2, [sui,std], BaseV1, Result(2));
//> 4: sui::package_config::record_minversion_upgrade(Input(1), Input(0), Result(3));
//> sui::package::commit_upgrade(Input(0), Result(3));

// The historical v1 reference selects v2 after the PTB commits.
//# run BaseV1::base::ping --sender A

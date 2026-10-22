// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Enrolled upgrades use the ordinary commit function, but only after their new selection has been
// recorded. A failed unrecorded commit leaves both the cap and PackageConfig unchanged.

//# init --addresses BaseV1=0x0 BaseV2=0x0 --accounts A

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

// The receipt cannot be committed until PackageConfig has recorded the upgrade.
//# programmable --sender A --inputs object(1,1) 0u8 digest(BaseV2)
//> 0: sui::package::authorize_upgrade(Input(0), Input(1), Input(2));
//> 1: Upgrade(BaseV2, [sui,std], BaseV1, Result(0));
//> sui::package::commit_upgrade(Input(0), Result(1));

// The failed transaction rolled back: the historical v1 package still executes its original code.
//# run BaseV1::base::ping --sender A

// The same cap can then complete the upgrade correctly.
//# programmable --sender A --inputs object(1,1) object(0xcf6) 0u8 digest(BaseV2)
//> 0: sui::package::authorize_upgrade(Input(0), Input(2), Input(3));
//> 1: Upgrade(BaseV2, [sui,std], BaseV1, Result(0));
//> 2: sui::package_config::record_minversion_upgrade(Input(1), Input(0), Result(1));
//> sui::package::commit_upgrade(Input(0), Result(1));

// The historical package ID now selects the upgraded package.
//# run BaseV1::base::ping --sender A

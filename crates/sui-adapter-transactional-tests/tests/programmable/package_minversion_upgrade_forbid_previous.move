// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Recording an enrolled upgrade atomically forbids its replaced version and updates minversion
// selection for the next transaction.

//# init --addresses BaseV1=0x0 BaseV2=0x0 --accounts A

//# publish --upgradeable --sender A
module BaseV1::base;
public fun ping() { abort 1 }

//# programmable --sender A --inputs object(1,1) object(0x426)
//> 0: sui::package::enable_minversion(Input(0));
//> 1: sui::package_config::record_minversion_enrollment(Input(1), Result(0));

//# stage-package
module BaseV2::base;
public fun ping() {}

//# programmable --sender A --inputs object(1,1) object(0x426) 0u8 digest(BaseV2)
//> 0: sui::package::prepare_minversion_upgrade(Input(0));
//> 1: sui::package::authorize_upgrade(Input(0), Input(2), Input(3));
//> 2: Upgrade(BaseV2, [sui,std], BaseV1, Result(1));
//> 3: sui::package::commit_minversion_upgrade(Input(0), Result(2), Result(0));
//> sui::package_config::record_minversion_upgrade_and_forbid_previous(Input(1), Result(3));

// v1 immediately resolves to v2, so the v1 forbid no longer applies.
//# run BaseV1::base::ping --sender A

//# set-address BaseV2 object(4,0)

// The newly upgraded package remains directly usable.
//# run BaseV2::base::ping --sender A

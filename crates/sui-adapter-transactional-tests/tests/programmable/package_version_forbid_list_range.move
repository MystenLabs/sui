// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Version-forbid ranges are inclusive and only accept historical versions.

//# init --addresses BaseV1=0x0 BaseV2=0x0 BaseV3=0x0 BaseV4=0x0 --accounts A

//# publish --upgradeable --sender A
module BaseV1::base;

public fun ping() {}

//# upgrade --package BaseV1 --upgrade-capability 1,1 --sender A
module BaseV2::base;

public fun ping() {}

//# upgrade --package BaseV2 --upgrade-capability 1,1 --sender A
module BaseV3::base;

public fun ping() {}

//# upgrade --package BaseV3 --upgrade-capability 1,1 --sender A
module BaseV4::base;

public fun ping() {}

// Both endpoints of the inclusive range are forbidden.
//# run sui::package_config::forbid_version_range --args object(0x426) object(1,1) 1 2 --sender A

//# run BaseV1::base::ping --sender A

//# run BaseV2::base::ping --sender A

// The adjacent historical version remains allowed.
//# run BaseV3::base::ping --sender A

// A range with its start above its end is invalid.
//# run sui::package_config::forbid_version_range --args object(0x426) object(1,1) 3 2 --sender A

// Version zero is not historical. The range must reject it rather than install a v0 entry.
//# run sui::package_config::forbid_version_range --args object(0x426) object(1,1) 0 1 --sender A

// The current version is not historical, so it cannot be forbidden.
//# run sui::package_config::forbid_version_range --args object(0x426) object(1,1) 1 4 --sender A

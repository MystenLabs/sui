// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A direct forbid beyond the forbidden prefix must not advance the watermark. Directly filling the
// gap must advance the watermark across the already-forbidden version.

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

// Establish a forbidden prefix through v1.
//# run sui::package_config::forbid_version --args object(0xcf6) object(1,1) 1 --sender A

// The bitmap and watermark both record v1.
//# view-object 5,0

//# view-object 5,1

// Directly forbid v3 while leaving a gap at v2. The watermark must remain at v1.
//# run sui::package_config::forbid_version --args object(0xcf6) object(1,1) 3 --sender A

// The bitmap records v1 and v3, but the watermark remains at v1.
//# view-object 5,0

//# view-object 5,1

// Directly filling the v2 gap must advance the watermark through the already-forbidden v3.
//# run sui::package_config::forbid_version --args object(0xcf6) object(1,1) 2 --sender A

// The bitmap and watermark now record a contiguous forbidden prefix through v3.
//# view-object 5,0

//# view-object 5,1

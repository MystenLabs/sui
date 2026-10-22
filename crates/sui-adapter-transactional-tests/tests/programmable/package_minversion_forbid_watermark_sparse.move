// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A sparse forbid range must not advance the contiguous-prefix watermark. Filling its gap must
// advance the watermark across the already-forbidden range.

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

// Only v2 is forbidden, leaving a gap at v1. This must not establish a forbidden prefix.
//# run sui::package_config::forbid_version_range --args object(0xcf6) object(1,1) 2 2 --sender A

// The first bitmap contains only the v2 bit; no watermark field was created.
//# view-object 4,0

// Filling v1 must advance the watermark through the already-forbidden v2 bit.
//# run sui::package_config::forbid_version --args object(0xcf6) object(1,1) 1 --sender A

// Both v1 and v2 are forbidden, and the watermark now records their contiguous prefix.
//# view-object 4,0

//# view-object 6,0

//# programmable --sender A --inputs object(1,1) object(0xcf6)
//> 0: sui::package::enable_minversion_permanently(Input(0));
//> 1: sui::package_config::record_minversion_enrollment(Input(1), Result(0));

// Enrollment selects v3 before the next upgrade.
//# view-object 9,0

//# stage-package
module BaseV4::base;
public fun ping() {}

// With the watermark already through v2, this operation only adds v3 to the existing bitmap.
//# programmable --sender A --inputs object(1,1) object(0xcf6) 0u8 digest(BaseV4)
//> 0: sui::package::authorize_upgrade(Input(0), Input(2), Input(3));
//> 1: Upgrade(BaseV4, [sui,std], BaseV3, Result(0));
//> 2: sui::package_config::record_minversion_upgrade_and_forbid_previous_versions(Input(1), Input(0), Result(1));
//> sui::package::commit_upgrade(Input(0), Result(1));

// The upgrade adds v3 to the bitmap and watermark and selects v4.
//# view-object 4,0

//# view-object 6,0

//# view-object 9,0

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Enrollment takes effect for the next transaction. A historical v1 reference selects the
// enrolled v2 package immediately after the enrollment transaction commits.

//# init --addresses BaseV1=0x0 BaseV2=0x0 BaseV3=0x0 --accounts A

//# publish --upgradeable --sender A
module BaseV1::base;
public fun ping() { abort 1 }

//# upgrade --package BaseV1 --upgrade-capability 1,1 --sender A
module BaseV2::base;
public fun ping() {}

// The policy write is not visible to another command in the same PTB: linkage uses the explicit
// root input's pre-state, so this historical v1 call still aborts and the PTB rolls back.
//# programmable --sender A --inputs object(1,1) object(0x426)
//> 0: sui::package::enable_minversion(Input(0));
//> 1: sui::package_config::record_minversion_enrollment(Input(1), Result(0));
//> 2: BaseV1::base::ping();

//# programmable --sender A --inputs object(1,1) object(0x426)
//> 0: sui::package::enable_minversion(Input(0));
//> 1: sui::package_config::record_minversion_enrollment(Input(1), Result(0));

// A second enrollment is rejected and leaves the original selection intact.
//# programmable --sender A --inputs object(1,1) object(0x426)
//> 0: sui::package::enable_minversion(Input(0));
//> 1: sui::package_config::record_minversion_enrollment(Input(1), Result(0));

// The enrolled selection immediately redirects v1 to v2.
//# run BaseV1::base::ping --sender A

// An explicit newer root is never downgraded to the selected package.
//# run BaseV2::base::ping --sender A

// An equal root remains v2 after selection.
//# run BaseV2::base::ping --sender A

// An enrolled cap cannot be permanently disabled. It can still perform a minversion upgrade.
//# run sui::package::disable_minversion_permanently --args object(1,1) --sender A

//# upgrade --package BaseV2 --upgrade-capability 1,1 --sender A --minversion
module BaseV3::base;
public fun ping() {}

//# run BaseV1::base::ping --sender A

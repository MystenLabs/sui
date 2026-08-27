// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// The setting's value (forbidden versions) is visible immediately to signing.

//# init --addresses BaseV1=0x0 BaseV2=0x0 TypeUser=0x0 --accounts A

//# publish --upgradeable --sender A
module BaseV1::base;

public struct Type has drop {}

public fun ping() {}

//# upgrade --package BaseV1 --upgrade-capability 1,1 --sender A
module BaseV2::base;

public struct Type has drop {}

public fun ping() {}

//# publish --sender A
module TypeUser::type_user;

public fun use_type<T>() {}

// The base package is allowed to be used before the forbid list is changed.
//# run BaseV1::base::ping --sender A

// The upgraded version is allowed before the forbid list is changed.
//# run BaseV2::base::ping --sender A

//# run sui::package_config::forbid_version --args object(0x426) object(1,1) 1 --sender A

// Signing reads the newer setting and immediately rejects version 1.
//# run BaseV1::base::ping --sender A

// Types from a forbidden package can still be used.
//# run TypeUser::type_user::use_type --type-args BaseV1::base::Type --sender A

// The newer resolved version remains allowed.
//# run BaseV2::base::ping --sender A

//# advance-epoch

// Remains rejected after the epoch change
//# run BaseV1::base::ping --sender A

// Types from a forbidden package remain usable after the epoch change.
//# run TypeUser::type_user::use_type --type-args BaseV1::base::Type --sender A

// The newer resolved version remains allowed after the epoch change.
//# run BaseV2::base::ping --sender A

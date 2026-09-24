// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// The setting's value (forbidden versions) is visible immediately to execution.

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

// The explicit package-config input is read at pre-state, so this PTB can still invoke v1.
//# programmable --sender A --inputs object(0x426) object(1,1) 1
//> 0: sui::package_config::forbid_version(Input(0), Input(1), Input(2));
//> 1: BaseV1::base::ping();

// The committed setting is visible to execution and rejects v1 in the next transaction.
//# run BaseV1::base::ping --sender A

// Types from a forbidden package can still be used.
//# run TypeUser::type_user::use_type --type-args BaseV1::base::Type --sender A

// The newer resolved version remains allowed.
//# run BaseV2::base::ping --sender A

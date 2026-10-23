// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Root was published against BaseV1, which has no dependencies. A type argument introduced by
// BaseV2 raises Root's executable Base dependency to v2, changing the runtime graph:
//
//   published graph:   Root -> BaseV1
//   selected graph:    Root -> BaseV2 -> NewV1
//
// The first combined call proves that NewV1 executes. After NewV1 is forbidden, using BaseV2 only
// as a type remains allowed, but combining that type with Root's call must reject the newly
// introduced executable dependency.

//# init --addresses NewV1=0x0 NewV2=0x0 BaseV1=0x0 BaseV2=0x0 Root=0x0 Generic=0x0 --accounts A

//# publish --upgradeable --sender A
module NewV1::new_dependency;
public struct Ping has copy, drop { version: u64 }
public fun call() { sui::event::emit(Ping { version: 1 }) }

//# publish --upgradeable --sender A
module BaseV1::base;
public fun call() {}

//# upgrade --package BaseV1 --upgrade-capability 2,1 --dependencies NewV1 --sender A
module BaseV2::base;
use NewV1::new_dependency;
public struct Marker has copy, drop {}
public fun call() { new_dependency::call() }

//# publish --dependencies BaseV1 --sender A
module Root::root;
use BaseV1::base;
public fun call() { base::call() }

//# publish --sender A
module Generic::generic;
public fun use_type<T>() {}

// The type argument raises Root's Base dependency to v2, so BaseV2 calls NewV1.
//# programmable --sender A
//> 0: Root::root::call();
//> 1: Generic::generic::use_type<BaseV2::base::Marker>();

// Make NewV1 historical, then forbid the version introduced only by BaseV2's dependency table.
//# upgrade --package NewV1 --upgrade-capability 1,1 --sender A
module NewV2::new_dependency;
public struct Ping has copy, drop { version: u64 }
public fun call() { sui::event::emit(Ping { version: 2 }) }

//# run sui::package_config::forbid_version --args object(0xcf6) object(1,1) 1 --sender A

// BaseV2 and its dependencies are type-only here, so the forbidden NewV1 remains allowed.
//# run Generic::generic::use_type --type-args BaseV2::base::Marker --sender A

// Root alone retains BaseV1, which has no New dependency.
//# run Root::root::call --sender A

// The same type argument now selects BaseV2 for Root. BaseV2 would execute forbidden NewV1, so the
// transaction must be rejected as InvalidLinkage.
//# programmable --sender A
//> 0: Root::root::call();
//> 1: Generic::generic::use_type<BaseV2::base::Marker>();

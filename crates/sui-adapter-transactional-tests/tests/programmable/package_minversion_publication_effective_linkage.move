// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A package records a flattened linkage table when it is published: one concrete package version
// for every dependency family in its transitive closure. Publication policy must validate that
// table because it describes the versions the new package will actually use. It must not recurse
// into each dependency's own historical table and validate those stale edges as well.
//
// For example, MidMin was published with LeafV1, but StaleMin explicitly declares LeafV2:
//
//   MidMin's table:       MidMin -> LeafV1  (now below the minimum)
//   StaleMin's table:     StaleMin -> MidMin
//                                  -> LeafV2
//
// StaleMin's effective flattened linkage uses LeafV2, so its publication is valid. Recursing into
// MidMin's table would incorrectly reject StaleMin because of LeafV1, even though LeafV1 is not in
// the new package's effective linkage.
//
// The opposite direction must use the same rule:
//
//   MidLatest's table:         MidLatest -> LeafV2
//   EffectiveStale's table:    EffectiveStale -> MidLatest
//                                             -> LeafV1  (below the minimum)
//
// EffectiveStale must be rejected because its own flattened linkage says it will use LeafV1. The
// LeafV2 entry in MidLatest's stored table does not replace the command's effective LeafV1 entry.
// This fixture covers both directions, forbidden versions, and both publish and upgrade commands.

//# init --addresses LeafV1=0x0 LeafV2=0x0 ForbiddenV1=0x0 ForbiddenV2=0x0 MidMin=0x0 MidForbidden=0x0 UpgradeV1=0x0 UpgradeV2=0x0 LatestNoInit=0x0 LatestWithInit=0x0 StaleMin=0x0 StaleForbidden=0x0 MidLatest=0x0 EffectiveStale=0x0 --accounts A

//# publish --upgradeable --sender A
module LeafV1::leaf;
public fun value(): u64 { 1 }

//# upgrade --package LeafV1 --upgrade-capability 1,1 --sender A
module LeafV2::leaf;
public fun value(): u64 { 2 }

//# publish --upgradeable --sender A
module ForbiddenV1::forbidden;
public fun value(): u64 { 1 }

//# upgrade --package ForbiddenV1 --upgrade-capability 3,1 --sender A
module ForbiddenV2::forbidden;
public fun value(): u64 { 2 }

//# publish --dependencies LeafV1 --sender A
module MidMin::mid;
use LeafV1::leaf;
public fun value(): u64 { leaf::value() }

//# publish --dependencies ForbiddenV1 --sender A
module MidForbidden::mid;
use ForbiddenV1::forbidden;
public fun value(): u64 { forbidden::value() }

//# publish --upgradeable --dependencies LeafV1 --sender A
module UpgradeV1::upgrade;
use LeafV1::leaf;
public fun value(): u64 { leaf::value() }

//# programmable --sender A --inputs object(1,1) object(0xcf6)
//> 0: sui::package::enable_minversion_permanently(Input(0));
//> sui::package_config::record_minversion_enrollment(Input(1), Result(0));

//# run sui::package_config::forbid_version --args object(0xcf6) object(3,1) 1 --sender A

// A no-init publish declaring the enrolled family's latest version succeeds.
//# publish --dependencies LeafV2 --sender A
module LatestNoInit::publisher;
use LeafV1::leaf;
public fun value(): u64 { leaf::value() }

// A publish with init also succeeds and observably executes against the latest dependency.
//# publish --dependencies LeafV2 --sender A
module LatestWithInit::publisher;
use LeafV1::leaf;
public struct Init has copy, drop { value: u64 }
fun init(_ctx: &mut sui::tx_context::TxContext) {
    sui::event::emit(Init { value: leaf::value() })
}

// MidMin's own table names below-minimum LeafV1, but this command's flattened linkage explicitly
// replaces it with LeafV2, so publication succeeds.
//# publish --dependencies MidMin LeafV2 --sender A
module StaleMin::publisher;
use MidMin::mid;
public fun value(): u64 { mid::value() }

// The same rule applies to a stale forbidden edge owned by MidForbidden.
//# publish --dependencies MidForbidden ForbiddenV2 --sender A
module StaleForbidden::publisher;
use MidForbidden::mid;
public fun value(): u64 { mid::value() }

// An upgrade with no new init is still publication policy and rejects a below-min dependency.
//# upgrade --package UpgradeV1 --upgrade-capability 7,1 --dependencies LeafV1 --sender A
module UpgradeV2::upgrade;
use LeafV1::leaf;
public fun value(): u64 { leaf::value() + 1 }

// The failed upgrade is atomic; declaring the latest enrolled dependency succeeds.
//# upgrade --package UpgradeV1 --upgrade-capability 7,1 --dependencies LeafV2 --sender A
module UpgradeV2::upgrade;
use LeafV2::leaf;
public fun value(): u64 { leaf::value() + 1 }

// MidLatest's own linkage uses the enrolled LeafV2.
//# publish --dependencies LeafV2 --sender A
module MidLatest::mid;
use LeafV1::leaf;
public fun value(): u64 { leaf::value() }

// The new package explicitly flattens the Leaf family back to LeafV1. Policy checks the new
// package's effective table, so this publish is rejected even though MidLatest's stored table uses
// LeafV2.
//# publish --dependencies MidLatest LeafV1 --sender A
module EffectiveStale::publisher;
use MidLatest::mid;
public fun value(): u64 { mid::value() }

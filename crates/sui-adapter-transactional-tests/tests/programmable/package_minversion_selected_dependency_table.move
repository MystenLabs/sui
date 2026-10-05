// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// When a dependency is selected through minversion, traversal must use the selected package's
// dependency table, not the historical package's flattened linkage closure.

//# init --addresses LeafV1=0x0 LeafV2=0x0 LeafV3=0x0 BaseV1=0x0 BaseV2=0x0 Consumer=0x0 --accounts A

//# publish --upgradeable --sender A
module LeafV1::leaf;
public fun value() { abort 1 }

//# upgrade --package LeafV1 --upgrade-capability 1,1 --sender A
module LeafV2::leaf;
public fun value() {}

//# publish --upgradeable --dependencies LeafV1 --sender A
module BaseV1::base;
use LeafV1::leaf;
public fun call() { leaf::value() }

//# upgrade --package BaseV1 --upgrade-capability 3,1 --dependencies LeafV2 --sender A
module BaseV2::base;
use LeafV2::leaf;
public fun call() { leaf::value() }

//# publish --dependencies BaseV1 LeafV1 --sender A
module Consumer::consumer;
use BaseV1::base;
public fun call() { base::call() }

//# programmable --sender A --inputs object(3,1) object(0x426)
//> 0: sui::package::enable_minversion(Input(0));
//> sui::package_config::record_minversion_enrollment(Input(1), Result(0));

// The selected BaseV2 dependency table immediately replaces BaseV1's table and uses LeafV2.
//# run Consumer::consumer::call --sender A

// BaseV1 is historical and can now be forbidden. Minversion selection happens first, so the
// Consumer's historical BaseV1 reference resolves to BaseV2 and remains executable.
//# run sui::package_config::forbid_version --args object(0x426) object(3,1) 1 --sender A

//# run Consumer::consumer::call --sender A

//# upgrade --package LeafV2 --upgrade-capability 1,1 --sender A
module LeafV3::leaf;
public fun value() {}

// LeafV2 is now historical. The selected BaseV2 package still executes LeafV2, so the forbid
// list rejects that selected executable dependency.
//# run sui::package_config::forbid_version --args object(0x426) object(1,1) 2 --sender A

//# run Consumer::consumer::call --sender A

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Port of `test_different_versions_across_calls` and `test_conflicting_versions_across_calls`
// from move_package_upgrade_tests. Both ultimately test the same thing: calling two versions
// of the same package within a single PTB. Two versions of the same package both define
// `return_0`; calling it once from each version pins that package to two distinct exact
// versions. Under unified linkage a transaction has a single linkage table, so this is
// rejected with InvalidLinkage.

//# init --addresses Test_V1=0x0 Test_V2=0x0 --accounts A

//# publish --upgradeable --sender A
module Test_V1::base {
    public fun return_0(): u64 { 0 }
}

//# upgrade --package Test_V1 --upgrade-capability 1,1 --sender A
module Test_V2::base {
    public fun return_0(): u64 { 0 }
}

//# programmable --sender A
//> Test_V1::base::return_0();
//> Test_V2::base::return_0();

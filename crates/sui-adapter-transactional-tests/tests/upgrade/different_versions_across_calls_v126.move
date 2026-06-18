// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Port of `test_different_versions_across_calls` from move_package_upgrade_tests, pinned to
// protocol version 126 (before unified linkage). With per-call linkage each MoveCall resolves
// its own package version independently, so calling `return_0` from two versions of the same
// package within a single PTB succeeds.

//# init --protocol-version 126 --addresses Test_V1=0x0 Test_V2=0x0 --accounts A

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

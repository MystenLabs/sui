// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Port of `test_conflicting_versions_across_calls` from move_package_upgrade_tests, pinned to
// protocol version 126 (before unified linkage). The base package's `return_0` aborts at v1 and
// returns normally at v2. A dependent package is published against base v1, then upgraded to
// depend on base v2. With per-call linkage each MoveCall resolves its own package version
// independently, so the call into the v2 dependent succeeds (base v2) while the call into the v1
// dependent runs base v1 and aborts with code 42 in the second command.

//# init --protocol-version 126 --addresses Base_V1=0x0 Base_V2=0x0 Dep_V1=0x0 Dep_V2=0x0 --accounts A

//# publish --upgradeable --sender A
module Base_V1::base {
    public fun return_0(): u64 { abort 42 }
}

//# publish --upgradeable --dependencies Base_V1 --sender A
module Dep_V1::my_module {
    use Base_V1::base;
    public fun call_return_0(): u64 { base::return_0() }
}

//# upgrade --package Base_V1 --upgrade-capability 1,1 --sender A
module Base_V2::base {
    public fun return_0(): u64 { 0 }
}

//# upgrade --package Dep_V1 --upgrade-capability 2,1 --dependencies Base_V2 --sender A
module Dep_V2::my_module {
    use Base_V2::base;
    public fun call_return_0(): u64 { base::return_0() }
}

//# programmable --sender A
//> Dep_V2::my_module::call_return_0();
//> Dep_V1::my_module::call_return_0();

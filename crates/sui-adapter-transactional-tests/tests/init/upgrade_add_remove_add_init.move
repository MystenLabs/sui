// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Add-remove-add: `a` has an `init` at v0, drops it at v1, then re-adds it at v2. The `init` check
// compares against the immediately-prior version, so at v2 module `a` (which had no `init` in
// v1) adding one again is treated as an existing module adding an `init`, and is rejected.

//# init --addresses v0=0x0 v1=0x0 v2=0x0 --accounts A

//# publish --upgradeable --sender A
module v0::a;
fun init(_: &mut TxContext) { }

//# upgrade --package v0 --upgrade-capability 1,1 --sender A
module v1::a;

//# upgrade --package v1 --upgrade-capability 1,1 --sender A
module v2::a;
fun init(_: &mut TxContext) {
    abort 0
}

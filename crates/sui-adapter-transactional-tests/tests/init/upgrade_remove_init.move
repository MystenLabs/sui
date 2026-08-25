// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Removing an `init` from an existing module during an upgrade is allowed: the upgrade succeeds and
// nothing is re-run.

//# init --addresses v0=0x0 v1=0x0 --accounts A

//# publish --upgradeable --sender A
module v0::a;
fun init(_: &mut TxContext) { }

//# upgrade --package v0 --upgrade-capability 1,1 --sender A
module v1::a;

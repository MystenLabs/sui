// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Exercises `sui::package::original_package_id`, which resolves the original
// (first-version) ID of the package an `UpgradeCap` authorizes. This needs a
// real published+upgraded package in storage, so it lives here rather than in
// the framework's Move unit tests.

//# init --addresses test_v1=0x0 test_v2=0x0 test_v3=0x0 solo=0x0 helper=0x0 --accounts A

//# publish --upgradeable --sender A
module test_v1::m {
    public fun f() { }
}

//# upgrade --package test_v1 --upgrade-capability 1,1 --sender A
module test_v2::m {
    public fun f() { }
}

//# publish --sender A
module helper::h {
    use sui::package::{Self, UpgradeCap};

    // Aborts (code 0) if the resolved original ID doesn't match `expected`.
    public entry fun check(cap: &UpgradeCap, expected: address) {
        assert!(package::original_package_id(cap).to_address() == expected, 0);
    }

    // Authorizing an upgrade sets `cap.package` to 0x0; reading the original
    // package ID while that upgrade is pending must abort (`EUpgradeInProgress`).
    public entry fun abort_mid_upgrade(cap: &mut UpgradeCap) {
        let _ticket = package::authorize_upgrade(cap, package::compatible_policy(), b"digest");
        let _id = package::original_package_id(cap);
        abort
    }
}

// Happy path after one upgrade: original ID must be the V1 package address.
//# run helper::h::check --args object(1,1) @test_v1 --sender A

//# upgrade --package test_v2 --upgrade-capability 1,1 --sender A
module test_v3::m {
    public fun f() { }
}

// Happy path after two upgrades: original ID is stable across versions.
//# run helper::h::check --args object(1,1) @test_v1 --sender A

// A package that was published but never upgraded is its own original: the
// resolved original ID equals the package's own address.
//# publish --upgradeable --sender A
module solo::m {
    public fun f() { }
}

//# run helper::h::check --args object(7,1) @solo --sender A

// Mid-upgrade: reading the original package ID with a pending (uncommitted)
// upgrade aborts with `EUpgradeInProgress` (5).
//# run helper::h::abort_mid_upgrade --args object(1,1) --sender A

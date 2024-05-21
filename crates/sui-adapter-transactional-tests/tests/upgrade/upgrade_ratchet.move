// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses V0=0x0 V1=0x0 V2=0x0 V3=0x0 V4=0x0 V5=0x0 --accounts A

//# publish --upgradeable --sender A
module V0::M1 {
    fun init(_ctx: &mut TxContext) { }
    public fun f1() { }
}

// Compatible -- this is fine
//# upgrade --package V0 --upgrade-capability 1,1 --sender A
module V1::M1 {
    public fun f1() { }
}

//# run sui::package::only_additive_upgrades --args object(1,1) --sender A

// Fails now since we've updated the package to only allow additive (or more restrictive) upgrades
//# upgrade --package V1 --upgrade-capability 1,1 --sender A --policy compatible
module V2::M1 {
    public fun f1() { }
}

// additive: this is fine
//# upgrade --package V1 --upgrade-capability 1,1 --sender A --policy additive
module V2::M1 {
    public fun f1() { }
}

// dep_only: this is fine
//# upgrade --package V2 --upgrade-capability 1,1 --sender A --policy dep_only
module V3::M1 {
    public fun f1() { }
}

//# run sui::package::only_dep_upgrades --args object(1,1) --sender A

// Fails now since we've updated the package to only allow dep_only  upgrades
//# upgrade --package V3 --upgrade-capability 1,1 --sender A --policy compatible
module V4::M1 {
    public fun f1() { }
}

// additive: this fails since it's < dep_only
//# upgrade --package V3 --upgrade-capability 1,1 --sender A --policy additive
module V4::M1 {
    public fun f1() { }
}

// dep_only: this is fine
//# upgrade --package V3 --upgrade-capability 1,1 --sender A --policy dep_only
module V4::M1 {
    public fun f1() { }
}

// Can't go back to a less restrictive policy
//# run sui::package::only_additive_upgrades --args object(1,1) --sender A

// Can make it immutable though
//# run sui::package::make_immutable --args object(1,1) --sender A

//# view-object 1,1

// Can't upgrade now -- upgrade cap is gone
//# upgrade --package V4 --upgrade-capability 1,1 --sender A --policy dep_only
module V5::M1 {
    public fun f1() { }
}

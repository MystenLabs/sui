// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Upgrading reserves the same MIST charge as publishing: fixed plus per module plus per
// dependency, 1_030_000_000 MIST for one module and two dependencies. The gas coin is viewed
// after each failure. Each failure runs out of gas, so the balance drops by the whole budget.

//# init --addresses test=0x0 u=0x0 u2=0x0 dep=0x0 --accounts A

//# publish --upgradeable --sender A
module test::m {
    public fun f() {}
}

//# publish --sender A
module dep::d {
    public fun f() {}
}

//# publish --upgradeable --sender A
module u::m {
    public fun f() {}
}

// An upgrade that adds a module and a dependency pays for both: two modules and three
// dependencies, 1_050_000_000 MIST. It runs first because the runner compiles a task with
// `--dependencies` against the sources of every earlier task.
//# upgrade --package u --upgrade-capability 3,1 --sender A --dependencies dep
module u2::m {
    public fun f() {}
}
module u2::n {
    public fun h() { dep::d::f() }
}

//# view-object 0,0

// A budget equal to the charge leaves nothing for execution: the whole budget is charged.
//# upgrade --package test --upgrade-capability 1,1 --sender A --gas-budget 1030000000
module test::m {
    public fun f() {}

    public fun g() {}
}

//# view-object 0,0

// A budget just above the charge succeeds.
//# upgrade --package test --upgrade-capability 1,1 --sender A --gas-budget 1100000000
module test::m {
    public fun f() {}

    public fun g() {}
}

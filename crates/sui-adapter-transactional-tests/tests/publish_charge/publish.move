// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Publishing reserves a MIST charge before execution: 1_000_000_000 fixed plus 10_000_000 per
// module plus 10_000_000 per dependency. With one module and two dependencies (std and sui) the
// charge is 1_030_000_000 MIST. A failed transaction prints no gas summary, so the gas coin is
// viewed after each failure. Each failure runs out of gas, so the balance drops by the whole budget.
// Each publish uses its own named address since a publish binds the address it used.

//# init --addresses t1=0x0 t2=0x0 t3=0x0 t4=0x0 t5=0x0 t6=0x0 --accounts A

// Default budget: the charge is part of the computation cost.
//# publish --sender A
module t1::m {
    public fun f() {}
}

// A budget just above the charge still covers the execution and storage.
//# publish --sender A --gas-budget 1100000000
module t2::m {
    public fun f() {}
}

//# view-object 0,0

// A budget equal to the charge leaves nothing for execution: out of gas, the whole budget is
// charged.
//# publish --sender A --gas-budget 1030000000
module t3::m {
    public fun f() {}
}

//# view-object 0,0

// A budget below the charge fails before the VM runs: the whole budget is charged.
//# publish --sender A --gas-budget 1000000000
module t3::m {
    public fun f() {}
}

//# view-object 0,0

// Three modules and three dependencies (std, sui, t1): 1_060_000_000 MIST.
//# publish --sender A --dependencies t1
module t4::a {
    public fun f() { t1::m::f() }
}
module t4::b {
    public fun f() {}
}
module t4::c {
    public fun f() {}
}

// The charge is in MIST, so a higher gas price raises only the execution part.
//# publish --sender A --gas-price 2000
module t5::m {
    public fun f() {}
}

// A dependency the code does not use is still charged: four dependencies, 1_050_000_000 MIST.
//# publish --sender A --dependencies t1 t4
module t6::m {
    public fun f() {}
}

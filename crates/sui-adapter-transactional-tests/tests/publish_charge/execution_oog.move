// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// The charge of 1_030_000_000 MIST fits in each budget below. A later phase runs out of gas in
// every task except the last one.
// Running out of gas in execution charges the whole budget. Running out of gas in storage keeps
// the charge and the execution and charges the gas coin storage only. The gas coin is viewed
// after each failure.

//# init --addresses t1=0x0 t2=0x0 t3=0x0 t4=0x0 --accounts A

//# view-object 0,0

// The budget covers the charge but not the minimum computation of 1_000_000 MIST.
//# publish --sender A --gas-budget 1030500000
module t1::m {
    public fun f() {}
}

//# view-object 0,0

// The budget equals the charge plus the minimum computation, which leaves no room: computation
// must stay below what the charge left.
//# publish --sender A --gas-budget 1031000000
module t1::m {
    public fun f() {}
}

//# view-object 0,0

// The init function loops until the execution runs out of gas.
//# publish --sender A --gas-budget 1100000000
module t2::m {
    fun init(_ctx: &mut TxContext) {
        loop {}
    }
}

//# view-object 0,0

// The charge and the computation fit. The storage of the package and the gas coin, 3_366_800 MIST,
// does not.
//# publish --sender A --gas-budget 1032000000
module t3::m {
    public fun f() {}
}

//# view-object 0,0

// The charge, the computation and the storage fit.
//# publish --sender A --gas-budget 1036000000
module t4::m {
    public fun f() {}
}

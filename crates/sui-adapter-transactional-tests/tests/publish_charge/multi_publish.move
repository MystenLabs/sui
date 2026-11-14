// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A PTB owes the charge of every Publish command before the first command runs. Each publish
// below has one module and two dependencies, 1_030_000_000 MIST, so three of them owe
// 3_090_000_000 MIST. A budget that covers the first one or two charges still fails before the
// VM runs. The gas coin is viewed after each failure. Each failure runs out of gas, so the balance
// drops by the whole budget.
// A Publish command in a programmable transaction binds no address, so every task reuses the
// staged packages.

//# init --addresses p1=0x0 p2=0x0 p3=0x0 spin=0x0 --accounts A

//# stage-package
module p1::m {
    public fun f() {}
}

//# stage-package
module p2::m {
    public fun f() {}
}

//# stage-package
module p3::m {
    public fun f() {}
}

//# stage-package
module spin::m {
    fun init(_ctx: &mut TxContext) {
        loop {}
    }
}

//# view-object 0,0

// The budget covers the first charge only.
//# programmable --sender A --inputs @A --gas-budget 2000000000
//> 0: Publish(p1, [sui, std]);
//> 1: Publish(p2, [sui, std]);
//> 2: Publish(p3, [sui, std]);
//> TransferObjects([Result(0), Result(1), Result(2)], Input(0))

//# view-object 0,0

// The budget covers the first two charges only.
//# programmable --sender A --inputs @A --gas-budget 3000000000
//> 0: Publish(p1, [sui, std]);
//> 1: Publish(p2, [sui, std]);
//> 2: Publish(p3, [sui, std]);
//> TransferObjects([Result(0), Result(1), Result(2)], Input(0))

//# view-object 0,0

// The budget equals the three charges, which leaves nothing for execution.
//# programmable --sender A --inputs @A --gas-budget 3090000000
//> 0: Publish(p1, [sui, std]);
//> 1: Publish(p2, [sui, std]);
//> 2: Publish(p3, [sui, std]);
//> TransferObjects([Result(0), Result(1), Result(2)], Input(0))

//# view-object 0,0

// The budget covers the three charges but not the minimum computation of 1_000_000 MIST.
//# programmable --sender A --inputs @A --gas-budget 3090500000
//> 0: Publish(p1, [sui, std]);
//> 1: Publish(p2, [sui, std]);
//> 2: Publish(p3, [sui, std]);
//> TransferObjects([Result(0), Result(1), Result(2)], Input(0))

//# view-object 0,0

// The charges fit, and the first publish runs out of gas in its init function.
//# programmable --sender A --inputs @A --gas-budget 3200000000
//> 0: Publish(spin, [sui, std]);
//> 1: Publish(p2, [sui, std]);
//> 2: Publish(p3, [sui, std]);
//> TransferObjects([Result(0), Result(1), Result(2)], Input(0))

//# view-object 0,0

// The charges fit, the first two publishes run, and the third runs out of gas in its init
// function.
//# programmable --sender A --inputs @A --gas-budget 3200000000
//> 0: Publish(p1, [sui, std]);
//> 1: Publish(p2, [sui, std]);
//> 2: Publish(spin, [sui, std]);
//> TransferObjects([Result(0), Result(1), Result(2)], Input(0))

//# view-object 0,0

// The budget covers the three charges and the execution: three packages are created.
//# programmable --sender A --inputs @A --gas-budget 3200000000
//> 0: Publish(p1, [sui, std]);
//> 1: Publish(p2, [sui, std]);
//> 2: Publish(p3, [sui, std]);
//> TransferObjects([Result(0), Result(1), Result(2)], Input(0))

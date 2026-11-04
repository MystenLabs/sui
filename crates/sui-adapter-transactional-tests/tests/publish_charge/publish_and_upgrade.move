// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A PTB that publishes and upgrades owes every charge before the first command runs:
// 1_030_000_000 MIST per command for one module and two dependencies. The gas coin is viewed
// after each failure. Each failure runs out of gas, so the balance drops by the whole budget.

//# init --addresses a0=0x0 a1=0x0 b0=0x0 b1=0x0 c0=0x0 c1=0x0 p=0x0 --accounts A

//# publish --upgradeable --sender A
module a0::m {
    public fun f() {}
}

//# publish --upgradeable --sender A
module b0::m {
    public fun f() {}
}

//# publish --upgradeable --sender A
module c0::m {
    public fun f() {}
}

//# stage-package
module a1::m {
    public fun f() {}

    public fun g() {}
}

//# stage-package
module b1::m {
    public fun f() {}

    public fun g() {}
}

//# stage-package
module c1::m {
    public fun f() {}

    public fun g() {}
}

//# stage-package
module p::m {
    public fun f() {}
}

//# view-object 0,0

// The budget covers the upgrade charge but not the publish charge after it.
//# programmable --sender A --inputs object(3,1) 0u8 digest(c1) @A --gas-budget 2000000000
//> 0: sui::package::authorize_upgrade(Input(0), Input(1), Input(2));
//> 1: Upgrade(c1, [sui, std], c0, Result(0));
//> 2: sui::package::commit_upgrade(Input(0), Result(1));
//> 3: Publish(p, [sui, std]);
//> TransferObjects([Result(3)], Input(3))

//# view-object 0,0

// The budget covers the charge of one of two upgrades.
//# programmable --sender A --inputs object(1,1) object(2,1) 0u8 digest(a1) digest(b1) --gas-budget 2000000000
//> 0: sui::package::authorize_upgrade(Input(0), Input(2), Input(3));
//> 1: Upgrade(a1, [sui, std], a0, Result(0));
//> 2: sui::package::commit_upgrade(Input(0), Result(1));
//> 3: sui::package::authorize_upgrade(Input(1), Input(2), Input(4));
//> 4: Upgrade(b1, [sui, std], b0, Result(3));
//> sui::package::commit_upgrade(Input(1), Result(4));

//# view-object 0,0

// The budget covers both upgrade charges and the execution.
//# programmable --sender A --inputs object(1,1) object(2,1) 0u8 digest(a1) digest(b1) --gas-budget 2100000000
//> 0: sui::package::authorize_upgrade(Input(0), Input(2), Input(3));
//> 1: Upgrade(a1, [sui, std], a0, Result(0));
//> 2: sui::package::commit_upgrade(Input(0), Result(1));
//> 3: sui::package::authorize_upgrade(Input(1), Input(2), Input(4));
//> 4: Upgrade(b1, [sui, std], b0, Result(3));
//> sui::package::commit_upgrade(Input(1), Result(4));

// The budget covers the upgrade and publish charges and the execution.
//# programmable --sender A --inputs object(3,1) 0u8 digest(c1) @A --gas-budget 2100000000
//> 0: sui::package::authorize_upgrade(Input(0), Input(1), Input(2));
//> 1: Upgrade(c1, [sui, std], c0, Result(0));
//> 2: sui::package::commit_upgrade(Input(0), Result(1));
//> 3: Publish(p, [sui, std]);
//> TransferObjects([Result(3)], Input(3))

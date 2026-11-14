// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A failed publish or upgrade still owes its charge: 1_030_000_000 MIST for one module and two
// dependencies. The charge counts the Publish and Upgrade commands of the PTB, so it is owed even
// when the PTB fails before such a command runs. The gas coin is viewed after each failure: the
// drop in its balance is the charge plus the execution and the gas coin storage. A failed upgrade
// also pays for the storage of the UpgradeCap it mutates.

//# init --addresses v0=0x0 v1=0x0 init_abort=0x0 p=0x0 dup=0x0 nodeps=0x0 --accounts A

//# publish --upgradeable --sender A
module v0::m {
    public fun f() {}
}

//# stage-package
module v1::m {
    public fun g() {}
}

//# stage-package
module p::m {
    public fun f() {}
}

//# stage-package
module dup::m {
    public fun f() {}
}

//# stage-package
module nodeps::m {
    public struct S has key {
        id: UID,
    }

    public fun new(ctx: &mut TxContext): S {
        S { id: object::new(ctx) }
    }
}

//# view-object 0,0

// The init function aborts.
//# publish --sender A
module init_abort::m {
    fun init(_ctx: &mut TxContext) {
        abort 0
    }
}

//# view-object 0,0

// The init function aborts at a gas price above the abort cap of 100 times the reference gas
// price of 500. The cap lowers the price of the execution to 50_000, and the MIST charge stays.
//# publish --sender A --gas-price 200000
module init_abort::m {
    fun init(_ctx: &mut TxContext) {
        abort 0
    }
}

//# view-object 0,0

// A command after the publish aborts.
//# programmable --sender A --inputs @A
//> 0: Publish(p, [sui, std]);
//> 1: TransferObjects([Result(0)], Input(0));
//> 2: MakeMoveVec<u64>([]);
//> std::vector::pop_back<u64>(Result(2));

//# view-object 0,0

// A command before the publish aborts, so the publish never runs.
//# programmable --sender A --inputs @A
//> 0: MakeMoveVec<u64>([]);
//> 1: std::vector::pop_back<u64>(Result(0));
//> 2: Publish(p, [sui, std]);
//> TransferObjects([Result(2)], Input(0))

//# view-object 0,0

// The upgrade ticket carries the digest of another package.
//# programmable --sender A --inputs object(1,1) 0u8 digest(p)
//> 0: sui::package::authorize_upgrade(Input(0), Input(1), Input(2));
//> 1: Upgrade(v1, [sui, std], v0, Result(0));
//> sui::package::commit_upgrade(Input(0), Result(1));

//# view-object 0,0

// The upgrade drops a public function, which the compatible policy rejects.
//# programmable --sender A --inputs object(1,1) 0u8 digest(v1)
//> 0: sui::package::authorize_upgrade(Input(0), Input(1), Input(2));
//> 1: Upgrade(v1, [sui, std], v0, Result(0));
//> sui::package::commit_upgrade(Input(0), Result(1));

//# view-object 0,0

// The publish lists no dependencies but uses sui: one module and no dependency, 1_010_000_000
// MIST.
//# programmable --sender A --inputs @A
//> 0: Publish(nodeps, []);
//> TransferObjects([Result(0)], Input(0))

//# view-object 0,0

// A dependency listed twice is charged twice: one module and three dependencies, 1_040_000_000
// MIST.
//# programmable --sender A --inputs @A
//> 0: Publish(dup, [sui, sui, std]);
//> TransferObjects([Result(0)], Input(0))

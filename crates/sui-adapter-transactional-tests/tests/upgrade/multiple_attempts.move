// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests the multiple attempts at upgrading a package with different layouts for a type
// in the new package

//# init --addresses v0=0x0 v1_0=0x0 v1_1=0x0 v2=0x0 --accounts A

//# publish --upgradeable --sender A
module v0::m {
}

//# stage-package
module v1_0::m {
}
module v1_0::n {
    public struct Obj has key, store {
        id: UID,
        f: u64,
    }
}

//# programmable --sender A --inputs 10 @A object(1,1) 0u8 digest(v1_0)
//> 0: sui::package::authorize_upgrade(Input(2), Input(3), Input(4));
//> 1: Upgrade(v1_0, [sui,std], v0, Result(0));
//> 2: sui::package::commit_upgrade(Input(2), Result(1));
//> 3: MakeMoveVec<u64>([]);
//> 4: std::vector::pop_back<u64>(Result(3));

//# upgrade --package v0 --upgrade-capability 1,1 --sender A
module v1_1::m {
}
module v1_1::n {
    public struct Obj has key, store {
        id: UID,
        f: u64,
        g: u64,
    }

    public fun new(ctx: &mut TxContext) {
        let obj = Obj {
            id: object::new(ctx),
            f: std::u64::max_value!(),
            g: std::u64::max_value!(),
        };
        transfer::public_transfer(obj, ctx.sender());
    }
}

//# programmable --sender A
//> v1_1::n::new();

//# view-object 5,0

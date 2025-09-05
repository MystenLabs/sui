// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests the multiple attempts at upgrading a package with different layouts for a type
// in the new package

//# init --addresses v0=0x0 v1=0x0 --accounts A

//# publish --upgradeable --sender A
module v0::m {
}

//# upgrade --package v0 --upgrade-capability 1,1 --sender A
// fail the upgrade via an abort in init
module v1::m {
}
module v1::n {
    public struct Obj has key, store {
        id: UID,
        f: u64,
    }

    fun init(ctx: &mut TxContext) {
        let obj = Obj {
            id: object::new(ctx),
            f: std::u64::max_value!(),
        };
        transfer::transfer(obj, ctx.sender());
        abort 0
    }
}

//# upgrade --package v0 --upgrade-capability 1,1 --sender A
module v1::m {
}
module v1::n {
    public struct Obj has key, store {
        id: UID,
        f: u64,
        g: u64,
    }

    fun init(ctx: &mut TxContext) {
        let obj = Obj {
            id: object::new(ctx),
            f: std::u64::max_value!(),
            g: std::u64::max_value!(),
        };
        transfer::transfer(obj, ctx.sender());
    }
}

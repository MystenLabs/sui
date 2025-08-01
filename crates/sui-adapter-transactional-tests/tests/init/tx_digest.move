// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests the correct tx digest used during init

//# init --addresses a=0x0 b=0x0  --accounts A

//# publish
module a::m {
    public struct Obj has key, store {
        id: UID,
        digest: vector<u8>
    }

    public fun new(ctx: &mut TxContext): Obj {
        Obj {
            id: object::new(ctx),
            digest: *ctx.digest(),
        }
    }

    public fun assert_same_digest(o1: &Obj, o2: &Obj) {
        assert!(&o1.digest == &o2.digest);
    }
}

//# stage-package
module b::m {
    fun init(ctx: &mut TxContext) {
        let o = a::m::new(ctx);
        transfer::public_transfer(o, ctx.sender());
    }
}

//# programmable --sender A --inputs @A
//> 0: Publish(b, [a, std, sui]);
//> 1: a::m::new();
//> TransferObjects([Result(0), Result(1)], Input(0));

//# programmable --sender A --inputs object(3,0) object(3,1)
//> a::m::assert_same_digest(Input(0), Input(1));

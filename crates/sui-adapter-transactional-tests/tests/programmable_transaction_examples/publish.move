// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// simple test of publish

//# init --addresses p=0x0 q=0x0 r=0x0 --accounts A

//# stage-package
module p::m {
    public fun foo(x: u64) {
        p::n::bar(x)
    }
}
module p::n {
    public fun bar(x: u64) {
        assert!(x == 0, 0);
    }
}


//# stage-package
module q::m {
    public fun x(): u64 { 0 }
}


//# programmable --sender A --inputs 10 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: Publish(q, []);
//> 2: TransferObjects([Result(0)], Input(1));
//> 3: Publish(p, []);
//> TransferObjects([Result(1), Result(3)], Input(1))

//# set-address p object(3,1)

//# set-address q object(3,0)

//# programmable --sender A
//> 0: q::m::x();
//> p::m::foo(Result(0))

//# publish --dependencies p q
module r::all {
    public fun foo_x() {
        p::m::foo(q::m::x())
    }
}

//# run r::all::foo_x

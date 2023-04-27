// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// simple test of upgrade

//# init --addresses p=0x0 q=0x0 q_2=0x0 r=0x0 s=0x0 --accounts A

//# publish
module p::m {
    public fun foo(x: u64) {
        p::n::bar(x)
    }
}
module p::n {
    public fun bar(x: u64) {
        assert!(x == 1, 0);
    }
}

//# publish --upgradeable --sender A
module q::m {
    public fun x(): u64 { 0 }
}

//# publish
module r::m {
    public fun y(): u64 { 0 }
}

//# stage-package --dependencies r
module q_2::m {
    public fun x(): u64 { y() + 1 }
    public fun y(): u64 { r::m::y() }
}

//# programmable --sender A --inputs 10 @A object(2,1) 0u8 digest(q_2)
//> 0: sui::package::authorize_upgrade(Input(2), Input(3), Input(4));
//> 1: SplitCoins(Gas, [Input(0)]);
//> 2: Upgrade(q_2, [sui,std,r], q, Result(0));
//> TransferObjects([Result(1)], Input(1));
//> sui::package::commit_upgrade(Input(2), Result(2))

//# programmable --sender A
//> 0: q::m::x();
//> p::m::foo(Result(0))

//# set-address q_2 object(5,0)

//# programmable --sender A
//> 0: q_2::m::x();
//> p::m::foo(Result(0))

//# publish --dependencies p q_2 r
module s::all {
    public fun foo_x() {
        p::m::foo(q::m::x())
    }
}

//# run s::all::foo_x

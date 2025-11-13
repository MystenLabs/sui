// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests returning a reference without any inputs

//# init --addresses test=0x0 --accounts A --allow-references-in-ptbs

//# publish
module test::m {

    public struct Pair has copy, drop, store{
        x: u64,
        y: u64,
    }

    public fun pair_mut(): &mut Pair { abort 0 }
    public fun box_mut(): (&mut Pair, &mut Pair) { abort 1 }

    public fun increment(p: &mut Pair) {
        p.x = p.x + 1;
        p.y = p.y + 1;
    }

    public fun swap_x(p1: &mut Pair, p2: &mut Pair) {
        let tmp = p1.x;
        p1.x = p2.x;
        p2.x = tmp;
    }
}


//# programmable
// This should be allowed and should abort
//> 0: test::m::pair_mut();
//> test::m::increment(Result(0));

//# programmable
// This should be allowed and should abort
//> 0: test::m::box_mut();
//> test::m::increment(NestedResult(0,0));
//> test::m::increment(NestedResult(0,1));
//> test::m::increment(NestedResult(0,0));
//> test::m::swap_x(NestedResult(0,0), NestedResult(0,1));

//# programmable
// This should be rejected by the borrow checker (in static PTBs)
//> 0: test::m::pair_mut();
//> test::m::swap_x(Result(0), Result(0));

//# programmable
// This should be rejected by the borrow checker (in static PTBs)
//> 0: test::m::box_mut();
//> test::m::swap_x(NestedResult(0,0), NestedResult(0,0));

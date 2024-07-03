// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests correct generic substitution in Move call

//# init --addresses test=0x0 --accounts A

//# publish
module test::m1 {
    public struct A has copy, drop { value: u64 }
    public struct B has copy, drop { value: u256 }

    public fun a(): A { A { value: 0 } }
    public fun b(): B { B { value: 0 } }

    public fun swap<T1: copy, T2: copy>(
        v1: &vector<T1>,
        v2: &mut vector<T2>,
    ): (vector<vector<T2>>, vector<vector<T1>>) {
        (vector[*v2], vector[*v1])
    }

    public fun eat(_: &vector<vector<A>>, _: &mut vector<vector<B>>, _: vector<vector<A>>) {}
}

// valid
//# programmable
//> 0: test::m1::a();
//> 1: test::m1::b();
//> 2: MakeMoveVec<test::m1::A>([Result(0)]);
//> 3: MakeMoveVec<test::m1::B>([Result(1)]);
//> 4: test::m1::swap<test::m1::A, test::m1::B>(Result(2), Result(3));
//  correct usage A                  B                  A
//> test::m1::eat(NestedResult(4,1), NestedResult(4,0), NestedResult(4,1));

// invalid
//# programmable
//> 0: test::m1::a();
//> 1: test::m1::b();
//> 2: MakeMoveVec<test::m1::A>([Result(0)]);
//> 3: MakeMoveVec<test::m1::B>([Result(1)]);
//> 4: test::m1::swap<test::m1::A, test::m1::B>(Result(2), Result(3));
//  incorrect usage B                B                  A
//> test::m1::eat(NestedResult(4,0), NestedResult(4,0), NestedResult(4,1));

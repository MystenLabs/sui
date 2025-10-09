// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests valid transfers/writes of mut references

//# init --addresses test=0x0 --accounts A --allow-references-in-ptbs

//# publish
module test::m {

    public struct Pair has copy, drop, store {
        x: u64,
        y: u64,
    }

    public fun pair(): Pair { Pair { x: 0 , y: 0 } }

    public fun borrow_mut<T>(t: &mut T): &mut T {
        t
    }

    public fun freeze_ref<T>(t: &mut T): &T {
        t
    }

    public fun use_ref<T>(_: &T) {
    }

    public fun write_pair(p: &mut Pair) {
        p.x = p.x + 1;
        p.y = p.y + 1;
    }

    public fun borrow_x_mut(p: &mut Pair): &mut u64 {
        &mut p.x
    }

    public fun borrow_x_y_mut(p: &mut Pair): (&mut u64, &mut u64) {
        (&mut p.x, &mut p.y)
    }

    public fun borrow_x_mut_y_imm(p: &mut Pair): (&mut u64, &u64) {
        (&mut p.x, &p.y)
    }

    public fun write_u64(p: &mut u64) {
        *p = *p + 1;
    }

}

// single mut child

//# programmable
// write to child, write to parent
//> 0: test::m::pair();
//> 1: test::m::borrow_x_mut(Result(0));
//> 2: test::m::write_u64(Result(1));
//> 3: test::m::write_pair(Result(0));

//# programmable
// read parent, write to child
//> 0: test::m::pair();
//> 1: test::m::borrow_x_mut(Result(0));
//> 2: test::m::use_ref<test::m::Pair>(Result(0));
//> 3: test::m::write_u64(Result(1));

//# programmable
// borrow parent, write to child, write to parent
//> 0: test::m::pair();
//> 1: test::m::borrow_mut<test::m::Pair>(Result(0));
//> 2: test::m::borrow_x_mut(Result(1));
//> 3: test::m::write_u64(Result(2));
//> test::m::write_pair(Result(1));

//# programmable
// borrow parent, read parent, write to child
//> 0: test::m::pair();
//> 1: test::m::borrow_mut<test::m::Pair>(Result(0));
//> 2: test::m::borrow_x_mut(Result(1));
//> 3: test::m::use_ref<test::m::Pair>(Result(1));
//> 4: test::m::write_u64(Result(2));
//> 5: test::m::use_ref<test::m::Pair>(Result(1));

// multiple mut children

//# programmable
// write to children, write to parent
//> 0: test::m::pair();
//> 1: test::m::borrow_x_y_mut(Result(0));
//> 2: test::m::write_u64(NestedResult(1,0));
//> 3: test::m::write_u64(NestedResult(1,1));
//> 4: test::m::write_pair(Result(0));

//# programmable
// borrow parent, write to children, read parent
//> 0: test::m::pair();
//> 1: test::m::borrow_mut<test::m::Pair>(Result(0));
//> 2: test::m::borrow_x_y_mut(Result(1));
//> 3: test::m::write_u64(NestedResult(2,0));
//> 4: test::m::write_u64(NestedResult(2,1));
//> 5: test::m::use_ref<test::m::Pair>(Result(1));

// mut child, imm child

//# programmable
// write to child, read child, write to parent
//> 0: test::m::pair();
//> 1: test::m::borrow_x_mut_y_imm(Result(0));
//> 2: test::m::write_u64(NestedResult(1,0));
//> 3: test::m::use_ref<u64>(NestedResult(1,1));
//> 4: test::m::write_u64(NestedResult(1,0));
//> 5: test::m::use_ref<u64>(NestedResult(1,1));
//> 6: test::m::write_pair(Result(0));

//# programmable
// borrow parent, write to child, read child, read parent
//> 0: test::m::pair();
//> 1: test::m::borrow_mut<test::m::Pair>(Result(0));
//> 2: test::m::borrow_x_mut_y_imm(Result(1));
//> 3: test::m::write_u64(NestedResult(2,0));
//> 4: test::m::use_ref<u64>(NestedResult(2,1));
//> 5: test::m::write_u64(NestedResult(2,0));
//> 6: test::m::use_ref<u64>(NestedResult(2,1));
//> 7: test::m::use_ref<test::m::Pair>(Result(1));

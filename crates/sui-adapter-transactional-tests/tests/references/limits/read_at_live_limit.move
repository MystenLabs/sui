// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun boom16(): (&mut u64, &mut u64, &mut u64, &mut u64, &mut u64, &mut u64, &mut u64, &mut u64, &mut u64, &mut u64, &mut u64, &mut u64, &mut u64, &mut u64, &mut u64, &mut u64) { abort 0 }
public fun id_imm<T>(t: &T): &T { t }
public fun check(x: u64, v: u64) { assert!(x == v, 0) }
public fun check_vec(v: vector<u64>, len: u64, value: u64) {
    assert!(v.length() == len, 0);
    v.do!(|x| assert!(x == value, 0));
}
public fun use_mut16(_: &mut u64, _: &mut u64, _: &mut u64, _: &mut u64, _: &mut u64, _: &mut u64, _: &mut u64, _: &mut u64, _: &mut u64, _: &mut u64, _: &mut u64, _: &mut u64, _: &mut u64, _: &mut u64, _: &mut u64, _: &mut u64) {}
public fun use_mut15(_: &mut u64, _: &mut u64, _: &mut u64, _: &mut u64, _: &mut u64, _: &mut u64, _: &mut u64, _: &mut u64, _: &mut u64, _: &mut u64, _: &mut u64, _: &mut u64, _: &mut u64, _: &mut u64, _: &mut u64) {}

//# programmable --inputs 0
// INVALID: InsufficientGas, "Command has 65 live references, exceeding the maximum of 64" at command 4, a read of a `&mut` that is used again
//> 0: test::m::boom16();
//> 1: test::m::boom16();
//> 2: test::m::boom16();
//> 3: test::m::boom16();
//> 4: test::m::check(NestedResult(0,0), Input(0));
//> 5: test::m::use_mut16(NestedResult(0,0), NestedResult(0,1), NestedResult(0,2), NestedResult(0,3), NestedResult(0,4), NestedResult(0,5), NestedResult(0,6), NestedResult(0,7), NestedResult(0,8), NestedResult(0,9), NestedResult(0,10), NestedResult(0,11), NestedResult(0,12), NestedResult(0,13), NestedResult(0,14), NestedResult(0,15));
//> 6: test::m::use_mut16(NestedResult(1,0), NestedResult(1,1), NestedResult(1,2), NestedResult(1,3), NestedResult(1,4), NestedResult(1,5), NestedResult(1,6), NestedResult(1,7), NestedResult(1,8), NestedResult(1,9), NestedResult(1,10), NestedResult(1,11), NestedResult(1,12), NestedResult(1,13), NestedResult(1,14), NestedResult(1,15));
//> 7: test::m::use_mut16(NestedResult(2,0), NestedResult(2,1), NestedResult(2,2), NestedResult(2,3), NestedResult(2,4), NestedResult(2,5), NestedResult(2,6), NestedResult(2,7), NestedResult(2,8), NestedResult(2,9), NestedResult(2,10), NestedResult(2,11), NestedResult(2,12), NestedResult(2,13), NestedResult(2,14), NestedResult(2,15));
//> 8: test::m::use_mut16(NestedResult(3,0), NestedResult(3,1), NestedResult(3,2), NestedResult(3,3), NestedResult(3,4), NestedResult(3,5), NestedResult(3,6), NestedResult(3,7), NestedResult(3,8), NestedResult(3,9), NestedResult(3,10), NestedResult(3,11), NestedResult(3,12), NestedResult(3,13), NestedResult(3,14), NestedResult(3,15));

//# programmable --inputs 0
// VALID: a read at the reference's last use at 64 live references, aborts at runtime in boom16
//> 0: test::m::boom16();
//> 1: test::m::boom16();
//> 2: test::m::boom16();
//> 3: test::m::boom16();
//> 4: test::m::check(NestedResult(0,0), Input(0));
//> 5: test::m::use_mut15(NestedResult(0,1), NestedResult(0,2), NestedResult(0,3), NestedResult(0,4), NestedResult(0,5), NestedResult(0,6), NestedResult(0,7), NestedResult(0,8), NestedResult(0,9), NestedResult(0,10), NestedResult(0,11), NestedResult(0,12), NestedResult(0,13), NestedResult(0,14), NestedResult(0,15));
//> 6: test::m::use_mut16(NestedResult(1,0), NestedResult(1,1), NestedResult(1,2), NestedResult(1,3), NestedResult(1,4), NestedResult(1,5), NestedResult(1,6), NestedResult(1,7), NestedResult(1,8), NestedResult(1,9), NestedResult(1,10), NestedResult(1,11), NestedResult(1,12), NestedResult(1,13), NestedResult(1,14), NestedResult(1,15));
//> 7: test::m::use_mut16(NestedResult(2,0), NestedResult(2,1), NestedResult(2,2), NestedResult(2,3), NestedResult(2,4), NestedResult(2,5), NestedResult(2,6), NestedResult(2,7), NestedResult(2,8), NestedResult(2,9), NestedResult(2,10), NestedResult(2,11), NestedResult(2,12), NestedResult(2,13), NestedResult(2,14), NestedResult(2,15));
//> 8: test::m::use_mut16(NestedResult(3,0), NestedResult(3,1), NestedResult(3,2), NestedResult(3,3), NestedResult(3,4), NestedResult(3,5), NestedResult(3,6), NestedResult(3,7), NestedResult(3,8), NestedResult(3,9), NestedResult(3,10), NestedResult(3,11), NestedResult(3,12), NestedResult(3,13), NestedResult(3,14), NestedResult(3,15));

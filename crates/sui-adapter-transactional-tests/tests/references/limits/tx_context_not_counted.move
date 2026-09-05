// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun refs17(v: &vector<u64>): (&u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64) { (&v[0], &v[1], &v[2], &v[3], &v[4], &v[5], &v[6], &v[7], &v[8], &v[9], &v[10], &v[11], &v[12], &v[13], &v[14], &v[15], &v[16]) }
public fun refs16(v: &vector<u64>): (&u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64) { (&v[0], &v[1], &v[2], &v[3], &v[4], &v[5], &v[6], &v[7], &v[8], &v[9], &v[10], &v[11], &v[12], &v[13], &v[14], &v[15]) }
public fun refs15(v: &vector<u64>): (&u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64) { (&v[0], &v[1], &v[2], &v[3], &v[4], &v[5], &v[6], &v[7], &v[8], &v[9], &v[10], &v[11], &v[12], &v[13], &v[14]) }
public fun refs12(v: &vector<u64>): (&u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64) { (&v[0], &v[1], &v[2], &v[3], &v[4], &v[5], &v[6], &v[7], &v[8], &v[9], &v[10], &v[11]) }
public fun refs11(v: &vector<u64>): (&u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64) { (&v[0], &v[1], &v[2], &v[3], &v[4], &v[5], &v[6], &v[7], &v[8], &v[9], &v[10]) }
public fun refs16_and_value(v: &vector<u64>): (&u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, u64) { (&v[0], &v[1], &v[2], &v[3], &v[4], &v[5], &v[6], &v[7], &v[8], &v[9], &v[10], &v[11], &v[12], &v[13], &v[14], &v[15], 0) }
public fun one(v: &vector<u64>): &u64 { &v[0] }
public fun boom16(): (&mut u64, &mut u64, &mut u64, &mut u64, &mut u64, &mut u64, &mut u64, &mut u64, &mut u64, &mut u64, &mut u64, &mut u64, &mut u64, &mut u64, &mut u64, &mut u64) { abort 0 }
public fun boom1(): &mut u64 { abort 0 }
public fun three_in_two_out(a: &u64, b: &u64, _c: &u64): (&u64, &u64) { (a, b) }
public fun use16(_: &u64, _: &u64, _: &u64, _: &u64, _: &u64, _: &u64, _: &u64, _: &u64, _: &u64, _: &u64, _: &u64, _: &u64, _: &u64, _: &u64, _: &u64, _: &u64) {}
public fun use15(_: &u64, _: &u64, _: &u64, _: &u64, _: &u64, _: &u64, _: &u64, _: &u64, _: &u64, _: &u64, _: &u64, _: &u64, _: &u64, _: &u64, _: &u64) {}
public fun use12(_: &u64, _: &u64, _: &u64, _: &u64, _: &u64, _: &u64, _: &u64, _: &u64, _: &u64, _: &u64, _: &u64, _: &u64) {}
public fun use11(_: &u64, _: &u64, _: &u64, _: &u64, _: &u64, _: &u64, _: &u64, _: &u64, _: &u64, _: &u64, _: &u64) {}
public fun use1(_: &u64) {}
public fun use_mut1(_: &mut u64) {}
public fun use_mut16(_: &mut u64, _: &mut u64, _: &mut u64, _: &mut u64, _: &mut u64, _: &mut u64, _: &mut u64, _: &mut u64, _: &mut u64, _: &mut u64, _: &mut u64, _: &mut u64, _: &mut u64, _: &mut u64, _: &mut u64, _: &mut u64) {}
public fun use_mut15(_: &mut u64, _: &mut u64, _: &mut u64, _: &mut u64, _: &mut u64, _: &mut u64, _: &mut u64, _: &mut u64, _: &mut u64, _: &mut u64, _: &mut u64, _: &mut u64, _: &mut u64, _: &mut u64, _: &mut u64) {}
public fun many_ctx(_: &TxContext, _: &TxContext, _: &TxContext, _: &TxContext, _: &TxContext, _: &TxContext, _: &TxContext, _: &TxContext, _: &TxContext, _: &TxContext, _: &TxContext, _: &TxContext, _: &TxContext, _: &TxContext, _: &TxContext, _: &TxContext, _: &TxContext, _: &TxContext, _: &TxContext, _: &TxContext) {}

//# programmable
// VALID: twenty `&TxContext` parameters at 64 live references, aborts at runtime in boom16
//> 0: test::m::boom16();
//> 1: test::m::boom16();
//> 2: test::m::boom16();
//> 3: test::m::boom16();
//> 4: test::m::many_ctx();
//> 5: test::m::use_mut16(NestedResult(0,0), NestedResult(0,1), NestedResult(0,2), NestedResult(0,3), NestedResult(0,4), NestedResult(0,5), NestedResult(0,6), NestedResult(0,7), NestedResult(0,8), NestedResult(0,9), NestedResult(0,10), NestedResult(0,11), NestedResult(0,12), NestedResult(0,13), NestedResult(0,14), NestedResult(0,15));
//> 6: test::m::use_mut16(NestedResult(1,0), NestedResult(1,1), NestedResult(1,2), NestedResult(1,3), NestedResult(1,4), NestedResult(1,5), NestedResult(1,6), NestedResult(1,7), NestedResult(1,8), NestedResult(1,9), NestedResult(1,10), NestedResult(1,11), NestedResult(1,12), NestedResult(1,13), NestedResult(1,14), NestedResult(1,15));
//> 7: test::m::use_mut16(NestedResult(2,0), NestedResult(2,1), NestedResult(2,2), NestedResult(2,3), NestedResult(2,4), NestedResult(2,5), NestedResult(2,6), NestedResult(2,7), NestedResult(2,8), NestedResult(2,9), NestedResult(2,10), NestedResult(2,11), NestedResult(2,12), NestedResult(2,13), NestedResult(2,14), NestedResult(2,15));
//> 8: test::m::use_mut16(NestedResult(3,0), NestedResult(3,1), NestedResult(3,2), NestedResult(3,3), NestedResult(3,4), NestedResult(3,5), NestedResult(3,6), NestedResult(3,7), NestedResult(3,8), NestedResult(3,9), NestedResult(3,10), NestedResult(3,11), NestedResult(3,12), NestedResult(3,13), NestedResult(3,14), NestedResult(3,15));

//# programmable
// VALID: twenty `&TxContext` parameters on their own
//> 0: test::m::many_ctx();

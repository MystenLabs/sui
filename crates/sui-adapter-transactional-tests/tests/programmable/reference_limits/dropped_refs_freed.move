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
public fun many_ctx(_: &TxContext, _: &TxContext, _: &TxContext, _: &TxContext, _: &TxContext, _: &TxContext, _: &TxContext, _: &TxContext, _: &TxContext, _: &TxContext, _: &TxContext, _: &TxContext, _: &TxContext, _: &TxContext, _: &TxContext, _: &TxContext, _: &TxContext, _: &TxContext, _: &TxContext, _: &TxContext) {}

//# programmable --inputs 0
// VALID: 65 commands each returning one unused reference
//> 0: MakeMoveVec<u64>([Input(0), Input(0), Input(0), Input(0), Input(0), Input(0), Input(0), Input(0), Input(0), Input(0), Input(0), Input(0), Input(0), Input(0), Input(0), Input(0), Input(0)]);
//> 1: test::m::one(Result(0));
//> 2: test::m::one(Result(0));
//> 3: test::m::one(Result(0));
//> 4: test::m::one(Result(0));
//> 5: test::m::one(Result(0));
//> 6: test::m::one(Result(0));
//> 7: test::m::one(Result(0));
//> 8: test::m::one(Result(0));
//> 9: test::m::one(Result(0));
//> 10: test::m::one(Result(0));
//> 11: test::m::one(Result(0));
//> 12: test::m::one(Result(0));
//> 13: test::m::one(Result(0));
//> 14: test::m::one(Result(0));
//> 15: test::m::one(Result(0));
//> 16: test::m::one(Result(0));
//> 17: test::m::one(Result(0));
//> 18: test::m::one(Result(0));
//> 19: test::m::one(Result(0));
//> 20: test::m::one(Result(0));
//> 21: test::m::one(Result(0));
//> 22: test::m::one(Result(0));
//> 23: test::m::one(Result(0));
//> 24: test::m::one(Result(0));
//> 25: test::m::one(Result(0));
//> 26: test::m::one(Result(0));
//> 27: test::m::one(Result(0));
//> 28: test::m::one(Result(0));
//> 29: test::m::one(Result(0));
//> 30: test::m::one(Result(0));
//> 31: test::m::one(Result(0));
//> 32: test::m::one(Result(0));
//> 33: test::m::one(Result(0));
//> 34: test::m::one(Result(0));
//> 35: test::m::one(Result(0));
//> 36: test::m::one(Result(0));
//> 37: test::m::one(Result(0));
//> 38: test::m::one(Result(0));
//> 39: test::m::one(Result(0));
//> 40: test::m::one(Result(0));
//> 41: test::m::one(Result(0));
//> 42: test::m::one(Result(0));
//> 43: test::m::one(Result(0));
//> 44: test::m::one(Result(0));
//> 45: test::m::one(Result(0));
//> 46: test::m::one(Result(0));
//> 47: test::m::one(Result(0));
//> 48: test::m::one(Result(0));
//> 49: test::m::one(Result(0));
//> 50: test::m::one(Result(0));
//> 51: test::m::one(Result(0));
//> 52: test::m::one(Result(0));
//> 53: test::m::one(Result(0));
//> 54: test::m::one(Result(0));
//> 55: test::m::one(Result(0));
//> 56: test::m::one(Result(0));
//> 57: test::m::one(Result(0));
//> 58: test::m::one(Result(0));
//> 59: test::m::one(Result(0));
//> 60: test::m::one(Result(0));
//> 61: test::m::one(Result(0));
//> 62: test::m::one(Result(0));
//> 63: test::m::one(Result(0));
//> 64: test::m::one(Result(0));
//> 65: test::m::one(Result(0));

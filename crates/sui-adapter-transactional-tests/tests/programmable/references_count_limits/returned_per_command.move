// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun refs16(v: &vector<u64>): (&u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64) { (&v[0], &v[1], &v[2], &v[3], &v[4], &v[5], &v[6], &v[7], &v[8], &v[9], &v[10], &v[11], &v[12], &v[13], &v[14], &v[15]) }

public fun refs16_and_value(v: &vector<u64>): (&u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, u64) { (&v[0], &v[1], &v[2], &v[3], &v[4], &v[5], &v[6], &v[7], &v[8], &v[9], &v[10], &v[11], &v[12], &v[13], &v[14], &v[15], 0) }

public fun use16(_: &u64, _: &u64, _: &u64, _: &u64, _: &u64, _: &u64, _: &u64, _: &u64, _: &u64, _: &u64, _: &u64, _: &u64, _: &u64, _: &u64, _: &u64, _: &u64) {}

//# programmable --inputs 0
// VALID: 16 references returned by one command
//> 0: MakeMoveVec<u64>([Input(0), Input(0), Input(0), Input(0), Input(0), Input(0), Input(0), Input(0), Input(0), Input(0), Input(0), Input(0), Input(0), Input(0), Input(0), Input(0), Input(0)]);
//> 1: test::m::refs16(Result(0));
//> 2: test::m::use16(NestedResult(1,0), NestedResult(1,1), NestedResult(1,2), NestedResult(1,3), NestedResult(1,4), NestedResult(1,5), NestedResult(1,6), NestedResult(1,7), NestedResult(1,8), NestedResult(1,9), NestedResult(1,10), NestedResult(1,11), NestedResult(1,12), NestedResult(1,13), NestedResult(1,14), NestedResult(1,15));

//# programmable --inputs 0
// VALID: 16 references and a value returned by one command
//> 0: MakeMoveVec<u64>([Input(0), Input(0), Input(0), Input(0), Input(0), Input(0), Input(0), Input(0), Input(0), Input(0), Input(0), Input(0), Input(0), Input(0), Input(0), Input(0), Input(0)]);
//> 1: test::m::refs16_and_value(Result(0));
//> 2: test::m::use16(NestedResult(1,0), NestedResult(1,1), NestedResult(1,2), NestedResult(1,3), NestedResult(1,4), NestedResult(1,5), NestedResult(1,6), NestedResult(1,7), NestedResult(1,8), NestedResult(1,9), NestedResult(1,10), NestedResult(1,11), NestedResult(1,12), NestedResult(1,13), NestedResult(1,14), NestedResult(1,15));

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun refs16(v: &vector<u64>): (&u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64, &u64) { (&v[0], &v[1], &v[2], &v[3], &v[4], &v[5], &v[6], &v[7], &v[8], &v[9], &v[10], &v[11], &v[12], &v[13], &v[14], &v[15]) }

//# programmable --inputs 0
// VALID: 256 references returned over the transaction, none used
//> 0: MakeMoveVec<u64>([Input(0), Input(0), Input(0), Input(0), Input(0), Input(0), Input(0), Input(0), Input(0), Input(0), Input(0), Input(0), Input(0), Input(0), Input(0), Input(0), Input(0)]);
//> 1: test::m::refs16(Result(0));
//> 2: test::m::refs16(Result(0));
//> 3: test::m::refs16(Result(0));
//> 4: test::m::refs16(Result(0));
//> 5: test::m::refs16(Result(0));
//> 6: test::m::refs16(Result(0));
//> 7: test::m::refs16(Result(0));
//> 8: test::m::refs16(Result(0));
//> 9: test::m::refs16(Result(0));
//> 10: test::m::refs16(Result(0));
//> 11: test::m::refs16(Result(0));
//> 12: test::m::refs16(Result(0));
//> 13: test::m::refs16(Result(0));
//> 14: test::m::refs16(Result(0));
//> 15: test::m::refs16(Result(0));
//> 16: test::m::refs16(Result(0));

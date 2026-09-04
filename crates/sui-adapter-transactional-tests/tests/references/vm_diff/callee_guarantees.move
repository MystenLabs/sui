// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// The PTB trusts that a callee's mutable results are disjoint: a callee that would return
// overlapping mutable references cannot be published.

//# init --addresses test=0x0 bad=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public struct Inner has store, copy, drop { f: u64, g: u64 }

public fun inner_val(): Inner { Inner { f: 0, g: 0 } }
public fun f_g_mut(i: &mut Inner): (&mut u64, &mut u64) { (&mut i.f, &mut i.g) }
public fun two_mut(a: &mut u64, b: &mut u64) { *a = 1; *b = 2 }
public fun check(r: &u64, v: u64) { assert!(*r == v, 0) }

//# publish --syntax mvir
// INVALID: the callee returns two mutable references to the same field, so the bytecode verifier rejects it
mvir bad::m {
    public struct Inner has copy, drop { f: u64, g: u64 }

    public fun same_twice(i: &mut Self::Inner): &mut u64 * &mut u64 {
    label b0:
        return (&mut copy(i).Inner::f, &mut move(i).Inner::f);
    }
}

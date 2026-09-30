// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// The PTB trusts that a callee's mutable results are disjoint: a callee that would return
// overlapping mutable references cannot be published.

//# init --addresses test=0x0 bad=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish --syntax mvir
// INVALID: the callee returns two mutable references to the same field, so the bytecode verifier rejects it
mvir bad::m {
    public struct Inner has copy, drop { f: u64, g: u64 }

    public fun same_twice(i: &mut Self::Inner): &mut u64 * &mut u64 {
    label b0:
        return (&mut copy(i).Inner::f, &mut move(i).Inner::f);
    }
}

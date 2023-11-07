module 0x1::StructSpecRelevance {
    struct Nonzero { i: u64, j: u64 }

    spec Nonzero {
        invariant i > 0;
    }

    // Can't leak i because it's involved in a spec
    public fun leak_i_bad(n: &mut Nonzero): &mut u64 {
        &mut n.i
    }

    // Leaking j is ok because specs say nothing about it
    public fun leak_j_ok(n: &mut Nonzero): &mut u64 {
        &mut n.j
    }

    public fun create(i: u64, j: u64): Nonzero {
        assert!(i > 0, 0);
        Nonzero { i, j }
    }
}

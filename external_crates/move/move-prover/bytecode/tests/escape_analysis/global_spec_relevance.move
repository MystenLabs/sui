module 0x1::GlobalSpecRelevance {
    invariant forall a: address where exists<Nonzero>(a): global<Nonzero>(a).i > 0;

    struct Nonzero has key { i: u64, j: u64 }

    // Can't leak i because it's involved in a spec
    public fun leak_i_bad(n: &mut Nonzero): &mut u64 {
        &mut n.i
    }

    // Leaking j is ok because specs say nothing about it
    public fun leak_j_ok(n: &mut Nonzero): &mut u64 {
        &mut n.j
    }

    public fun create(i: u64, j: u64): Nonzero {
        Nonzero { i, j }
    }

    public fun publish(account: &signer, n: Nonzero) {
        assert!(n.i > 0, 0);
        move_to(account, n)
    }
}

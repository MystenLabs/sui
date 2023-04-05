// dep: ../../move-stdlib/sources/vector.move

module 0x1::VecEq {
    use std::vector;

    struct G { v: vector<u64> }

    spec G {
        invariant v == vec<u64>(10);
    }

    public fun new(): G {
        G { v: vector::singleton(10) }
    }

    // should complain
    public fun leak_v(g: &mut G): &mut vector<u64> {
        &mut g.v
    }

    // should also complain
    public fun leak_index_into_v(g: &mut G): &mut u64 {
        vector::borrow_mut(&mut g.v, 0)
    }
}

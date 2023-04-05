module 0x42::struct_invariant_mut_ref_param {
    use std::vector;

    struct S {
        v: vector<u64>,
    }
    spec S {
        invariant len(v) == 0;
    }

    public fun empty(): S {
        S { v: vector::empty<u64>() }
    }

    public fun push_1(s: &mut S) {
        spec {
            assert len(s.v) == 0;
        };
        vector::push_back(&mut s.v, 1);
    }

    public fun push_2(s: &mut S) {
        spec {
            assert len(s.v) == 0;
        };
        vector::push_back(&mut s.v, 2);
        let t = freeze(s);
        let _ = vector::length(&t.v);
    }

    public fun push_3(s: &mut S): &mut S {
        spec {
            assert len(s.v) == 0;
        };
        vector::push_back(&mut s.v, 3);
        s
    }

    public fun push_and_pop_correct_1(s: &mut S) {
        spec {
            assert len(s.v) == 0;
        };
        vector::push_back(&mut s.v, 0);

        // NOTE: data invariant violation for `&mut` param is allowed within
        // function body (in this example, the data invariant does not hold
        // in the time window between push and pop).
        //
        // Implementation-wise, a data invariant is enforced when a `&mut` param
        // gets destroyed instead of when a `&mut` param is written-back to.

        vector::pop_back(&mut s.v);
    }

    public fun push_and_pop_correct_2(s: &mut S) {
        spec {
            assert len(s.v) == 0;
        };
        let v = &mut s.v;
        vector::push_back(v, 0);
        vector::pop_back(v);
    }

    struct A {
        v1: vector<u64>,
        v2: vector<u64>,
    }
    spec A {
        invariant len(v1) == len(v2);
    }

    public fun push_A_in_sync(a: &mut A) {
        let v1 = &mut a.v1;
        let v2 = &mut a.v2;
        vector::push_back(v1, 42);
        vector::push_back(v2, 42);
    }
}

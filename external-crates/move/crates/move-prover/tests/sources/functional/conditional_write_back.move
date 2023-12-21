module 0x42::Test {
    struct S {
        x: u64,
        y: u64,
    }

    struct T has key {
        x: u64,
    }

    struct R has key {
        x: u64,
    }

    public fun diff_field(cond: bool): S {
        let s = S { x: 0, y: 0  };

        let f = if (cond) {
            &mut s.x
        } else {
            &mut s.y
        };
        *f = 1;

        s
    }
    spec diff_field {
        ensures cond ==> (result.x == 1 && result.y == 0);
        ensures (!cond) ==> (result.x == 0 && result.y == 1);
    }

    public fun diff_address(cond: bool, a1: address, a2: address) acquires T {
        let x = if (cond) {
            let t1 = borrow_global_mut<T>(a1);
            &mut t1.x
        } else {
            let t2 = borrow_global_mut<T>(a2);
            &mut t2.x
        };
        *x = 0;
    }
    spec diff_address {
        aborts_if cond && !exists<T>(a1);
        aborts_if !cond && !exists<T>(a2);
        ensures if (cond) global<T>(a1).x == 0 else global<T>(a2).x == 0;
    }

    public fun diff_location(cond: bool, a: address, l: &mut T) acquires T {
        let x = if (cond) {
            let t1 = borrow_global_mut<T>(a);
            &mut t1.x
        } else {
            let t2 = l;
            &mut t2.x
        };
        *x = 0;
    }
    spec diff_location {
        aborts_if cond && !exists<T>(a);
        ensures if (cond) global<T>(a).x == 0 else l.x == 0;
    }

    public fun diff_resource(cond: bool, a: address) acquires T, R {
        let x = if (cond) {
            let t1 = borrow_global_mut<T>(a);
            &mut t1.x
        } else {
            let t2 = borrow_global_mut<R>(a);
            &mut t2.x
        };
        *x = 0;
    }
    spec diff_resource {
        aborts_if cond && !exists<T>(a);
        aborts_if !cond && !exists<R>(a);
        ensures if (cond) global<T>(a).x == 0 else global<R>(a).x == 0;
    }

    struct V<T: store> has key {
        x: u64,
        y: T,
    }

    public fun diff_resource_generic<A: store, B: store>(cond: bool, a: address) acquires V {
        let x = if (cond) {
            let t1 = borrow_global_mut<V<A>>(a);
            &mut t1.x
        } else {
            let t2 = borrow_global_mut<V<B>>(a);
            &mut t2.x
        };
        *x = 0;
    }
    spec diff_resource_generic {
        aborts_if cond && !exists<V<A>>(a);
        aborts_if !cond && !exists<V<B>>(a);
        ensures if (cond) global<V<A>>(a).x == 0 else global<V<B>>(a).x == 0;
    }

    public fun diff_local_simple(cond: bool) {
        let v1 = 0;
        let v2 = 0;

        let r = if (cond) { &mut v1  } else { &mut v2  };
        *r = 1;

        spec {
            assert (cond)  ==> (v1 == 1 && v2 == 0);
            assert (!cond) ==> (v1 == 0 && v2 == 1);
        }
    }

    public fun diff_local_global_mix_simple(cond: bool) acquires T {
        let t = T { x : 0  };
        let r = if (cond) { borrow_global_mut<T>(@0x1) } else { &mut t  };
        r.x = 1;

        spec {
            assert (cond)  ==> t.x == 0;
            assert (!cond) ==> t.x == 1;
        };

        let T { x : _ } = t;
    }
    spec diff_local_global_mix_simple {
        aborts_if cond && !exists<T>(@0x1);
        ensures (cond)  ==> global<T>(@0x1).x == 1;
        ensures (!cond) ==> global<T>(@0x1).x == old(global<T>(@0x1).x);
    }
}

module 0x42::MovedLocalWithRefs {
    use std::vector;

    struct S has drop {
        x: u64,
    }

    fun moved_local_in_loop(length: u64, limit: u64): vector<S> {
        let v = vector::empty();
        let i = 0;
        while ({
            spec {
                invariant i <= length;
                invariant len(v) == i;
                invariant forall k in 0..i: v[k].x <= limit;
            };
            (i < length)
        }) {
            let s = S { x : 100 };
            if (s.x >= limit) {
                s.x = limit;
            };
            vector::push_back(&mut v, s);
            i = i + 1;
        };
        v
    }

    spec moved_local_in_loop {
        ensures len(result) == length;
        ensures forall e in result: e.x <= limit;
    }
}

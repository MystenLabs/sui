module 0x1::StructEq {

    struct S { f: u64 }

    invariant forall s: S: s == S { f: 10 };

    public fun new(): S {
        S { f: 10 }
    }

    // should complain
    public fun leak_f(s: &mut S): &mut u64 {
        &mut s.f
    }
}

module 0x6::StructEq {

    public struct S { f: u64 }

    public fun new(): S {
        S { f: 10 }
    }

    // should complain
    public fun leak_f(s: &mut S): &mut u64 {
        &mut s.f
    }
}

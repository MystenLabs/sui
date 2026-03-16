// Tests that #[allow(...)] works on macros called via method syntax.

module a::m {
    public struct S has copy, drop { value: u64 }

    #[allow(unused_variable)]
    public macro fun do_thing($self: S): u64 {
        let unused = 0u64;
        let s = $self;
        s.value
    }

    fun call_it(): u64 {
        let s = S { value: 42 };
        s.do_thing!()
    }
}

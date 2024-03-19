module 0x42::m {

    struct S {}

    #[syntax(index)]
    public fun index_s(s: &S): &S { s }

    #[syntax(index)]
    public fun index_mut_s(s: &mut S, _i: u64): &mut S { s }

}

module 0x42::m {

    public struct S {}

    #[syntax(index)]
    public fun index_s(s: &S): &S { s }

    #[syntax(index)]
    public fun index_s2(s: &S): &S { s }

}

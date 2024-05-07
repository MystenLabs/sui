module 0x42::a {

    public struct S has drop {}

    #[syntax(index)]
    public fun index_s(_s: S, i: &u64): &u64 { i }

    #[syntax(index)]
    public fun index_t<T: drop>(_s: T, i: &u64): &u64 { i }

    #[syntax(index)]
    public fun index_multi_s(_mutli_s: (S,S), i: &u64): &u64 { i }

    #[syntax(index)]
    public fun index_unit(_unit: (), i: &u64): &u64 { i }

    #[syntax(index)]
    public fun index_no_arg(): &u64 { abort 0 }

}

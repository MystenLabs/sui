module 0x42::a {

    public struct S has drop {}

    #[syntax(index)]
    public fun index_s(_s: &S): |u64| -> u64 { abort 0 }

    #[syntax(index)]
    public fun index_s_mut(_s: &mut S): () { abort 0 }

}

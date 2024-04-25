module 0x42::a {

    public struct S has drop {}

    #[syntax(index)]
    public fun index_s(_s: &S): |u64| -> u64 { abort 0 }

    #[syntax(index)]
    public fun index_s_mut(_s: &mut S): () { abort 0 }

    public struct T has drop {}

    #[syntax(index)]
    public fun index_t(_t: &T): &(|u64| -> u64) { abort 0 }

    #[syntax(index)]
    public fun index_t_mut(_t: &mut T): &mut () { abort 0 }

}

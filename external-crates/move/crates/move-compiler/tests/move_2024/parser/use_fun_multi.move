module a::m {
    use fun foo as (u64, bool).foo;
    fun foo(_: u64, _: bool) { abort 0 }
}

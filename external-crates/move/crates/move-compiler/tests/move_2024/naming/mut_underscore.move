module a::m {
    // meaningless to have mut _
    fun foo(mut _: u64) {
        let mut _ = 0;
    }
}

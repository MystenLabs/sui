module a::m {
    // suppress unused mut with a leading _
    fun foo(mut _x: u64) {
        let mut _y = 0;
    }
}

module a::m {
    public fun foo(_: u64) {}
    fun bar() {
        foo!(|| ())
    }
}

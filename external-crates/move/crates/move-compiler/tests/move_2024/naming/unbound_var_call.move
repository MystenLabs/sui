module a::m {
    fun foo() {}

    fun unbound(fooo: u64) {
        // unbound function/variable fo
        fo();
        fooo;
    }
}

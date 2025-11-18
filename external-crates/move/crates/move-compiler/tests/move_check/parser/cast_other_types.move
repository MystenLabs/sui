module a::m {
    fun t() {
        // we only allow numeric types
        (0u64 as ());
        (0u64 as &u64);
    }
}

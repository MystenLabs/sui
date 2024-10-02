module a::m {
    fun t() {
        // we only allow numeric types
        (0 as ());
        (0 as &u64);
    }
}

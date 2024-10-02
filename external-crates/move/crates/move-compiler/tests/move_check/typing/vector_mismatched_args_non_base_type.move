module 0x42::Test {
    struct X has drop {}
    struct Y has drop {}

    fun t() {
        // test args of incompatible types
        vector<&mut u64>[&0];
        vector[(), (0, 1)];
    }
}

module 0x42::Test {
    struct X has drop {}
    struct Y has drop {}

    fun t() {
        // test args of incompatible types
        vector[0u64, false];
        vector[0u8, 0u64, 0u128];
        vector[0u64, @0];
        vector[X{}, Y{}];
        vector[&0u64, &false];
    }
}

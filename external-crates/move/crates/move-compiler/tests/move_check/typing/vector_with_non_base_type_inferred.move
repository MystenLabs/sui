module 0x42::Test {
    fun t() {
        // test invalid vector instatiation, inferred type
        vector[&0u64];
        vector[&mut 0u64];
        vector[()];
        vector[(0u64, false)];
        // tests valid subtyping join... although not important at the moment
        vector[&0, &mut 0u64];
        vector[&mut 0u64, &0];
    }
}

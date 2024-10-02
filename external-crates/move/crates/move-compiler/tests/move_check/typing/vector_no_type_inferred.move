module 0x42::Test {
    fun t() {
        // should error as it cannot infer a type
        vector[];
    }
}

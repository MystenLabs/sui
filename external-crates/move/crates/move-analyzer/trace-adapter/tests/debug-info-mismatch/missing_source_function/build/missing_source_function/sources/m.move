module missing_source_function::m {
    fun test() {
        missing_from_source();
        let _after_call = 0;
    }
}

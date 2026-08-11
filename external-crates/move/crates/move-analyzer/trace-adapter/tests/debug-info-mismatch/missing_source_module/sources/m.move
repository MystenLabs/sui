module missing_source_module::m {
    use missing_source_module::m2;

    fun test() {
        m2::missing_from_source();
        let _after_call = 0;
    }
}

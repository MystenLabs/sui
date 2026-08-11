module missing_code_map_source_file::m {
    fun test() {
        bad_source_map();
        let _after_call = 0;
    }

    fun bad_source_map() {
        let _x = 0;
    }
}

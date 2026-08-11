module missing_inlined_source_map::m {
    use missing_inlined_source_map::m_valid::valid_inline_marker;
    use missing_inlined_source_map::m2::inlined_source_marker;

    fun test() {
        bad_inline_map();
        let _after = 1;
    }

    fun bad_inline_map() {
        let _before_bad = 2;
        bar_before();
        inlined_source_marker!();
        baz_after();
        let _after_missing = 4;
    }

    fun bar_before() {
        let _before_before = 5;
        valid_inline_marker!();
        let _after_before = 3;
    }

    fun baz_after() {
        let _before_after = 6;
        valid_inline_marker!();
        let _after_after = 5;
    }
}

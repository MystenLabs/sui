// This is a test based on the example in the unit testing proposal
module 0x6::TestonlyModule {
    #[test_only]
    public fun aborts() {
        abort 42
    }
}

module 0x7::Module {
    fun a(a: u64): bool {
        a == 10
    }

    fun aborts() {
       abort 10
    }

    ///////////////////////////////////////////////////////////////////////////
    // unit tests
    ///////////////////////////////////////////////////////////////////////////

    // A test-only module import
    #[test_only]
    use 0x6::TestonlyModule;

    // A test only struct. This will only be included in test mode.
    #[test_only, allow(unused_field)]
    public struct C<T> has drop, key, store { x: T }

    #[test] // test entry point.
    fun tests_a() { // an actual test that will be run
        assert!(a(0) == false, 0);
        assert!(a(10), 1);
    }

    // check that this test aborts with the expected error code
    #[test]
    #[expected_failure(abort_code=10, location=Self)]
    fun tests_aborts() {
       aborts()
    }

    #[test]
    #[expected_failure(abort_code=42, location=0x6::TestonlyModule)]
    fun other_module_aborts() {
       TestonlyModule::aborts()
    }
}

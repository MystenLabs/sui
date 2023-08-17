module 0x42::used_consts {

    // at this point consts can only be used in functions and annotations (and not, for example, to
    // define other constants) so we can only test for that

    const USED_IN_FUN: u64 = 42;
    const USED_IN_ANNOTATION: u64 = 42;

    public fun foo(): u64 {
        USED_IN_FUN
    }

    #[test(p = @42)]
    #[expected_failure(abort_code = USED_IN_ANNOTATION)]
    public fun baz(p: u64) {
        assert!(p > 7, 42);
    }


}

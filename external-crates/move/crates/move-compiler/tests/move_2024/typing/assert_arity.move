module 0x0::m {
    fun t() {
        // test that all arities of assert do not cause compiler panics
        assert!();
        assert!(true);
        assert!(true, 1);
        assert!(true, 1, 2u64);

        // A small use after move error to make sure we get this far in the compiler
        let x = 0u64;
        move x;
        move x;
    }
}

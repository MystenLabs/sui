module 0x42::m {
    fun foo_1() {
        foo_2();
    }

    fun foo_2() {
        foo_3();
    }

    fun foo_3() {
        foo_1();
    }
    spec foo_3 {
        // NOTE: our current verification setup
        // requires at least one function in a
        // recursive function group to be marked
        // as opaque (with preferably a complete
        // set of aborts and ensures conditions).
        //
        // In this case, at least one of
        // - foo_1()
        // - foo_2()
        // - foo_3()
        // must be marked as opaque
        //
        // This is simiar to requiring annotations
        // of loop invariants to break the cycles.

        pragma opaque;
    }
}

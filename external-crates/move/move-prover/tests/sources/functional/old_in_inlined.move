module 0x42::OldInInlined {

    // tests the case when an inlined spec uses "old" and a function containing it (bar in this
    // case) is itself called from another function (foo in this case) - "old" value must be
    // recorded not only in the "verified" (bar) function variant but also in the baseline variant
    // if it's called from another function

    fun foo() {
        let x = 0;
        bar(&mut x);
    }

    fun bar(y: &mut u64) {
        *y = *y + 2;
        spec {
            assert y > old(y) + 1;
        }
    }
}

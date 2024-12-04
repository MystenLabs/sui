module 0x42::excessive_nesting_true_negative {

    #[allow(unused_assignment)]
    fun flat_code(x: u64) {
        if (x > 10) {
            x = x + 1;
        };
        if (x > 20) {
            x = x + 2;
        };
        if (x > 30) {
            x = x + 3;
        }
    }

    #[allow(unused_assignment)]
    fun acceptable_nesting(x: u64) {
        if (x > 0) {
            if (x > 10) {
                if (x > 20) { // At MAX_NESTING_LEVEL (3)
                    x = x + 1;
                }
            }
        }
    }
}

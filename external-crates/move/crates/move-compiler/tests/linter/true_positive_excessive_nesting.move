module 0x42::excessive_nesting_true_positive {

    #[allow(unused_assignment)]
    fun deeply_nested_if(x: u64) {
        if (x > 0) {
            if (x > 10) {
                if (x > 20) {
                    if (x > 30) { // This exceeds MAX_NESTING_LEVEL (3)
                        x = x + 1;
                    }
                }
            }
        }
    }

    fun deeply_nested_loops(x: u64) {
        while (x > 0) {
            while (x > 10) {
                while (x > 20) {
                    loop { // This exceeds MAX_NESTING_LEVEL (3)
                        if (x == 0) break;
                        x = x - 1;
                    }
                }
            }
        }
    }
}

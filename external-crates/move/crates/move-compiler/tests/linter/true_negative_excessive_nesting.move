module 0x42::excessive_nesting_true_negative {
    public fun shallow_nesting(x: u64) {
        if (x > 0) {
            {
                // This is only the 2nd level of nesting
                let y = x + 1;
                if (y > 10) {
                    // This is the 3rd level, which is at the threshold but not exceeding it
                };
            }
        };
    }

    public fun multiple_shallow_blocks(x: u64) {
        {
            // 1st level
        };
        {
            {
                // 2nd level
            }
        };
        if (x > 5) {
            {
                // Still only 2nd level
            }
        };
    }
}

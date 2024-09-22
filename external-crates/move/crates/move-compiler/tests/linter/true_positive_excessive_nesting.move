module 0x42::excessive_nesting_true_positive {
    public fun deeply_nested_function(x: u64) {
        if (x > 0) {
            {
                {
                    {
                        {
                            // This is the 5th level of nesting, which exceeds the threshold of 3
                            let y = x + 1;
                            if (y > 10) {
                                {
                                    // Even deeper nesting
                                }
                            };
                        }
                    }
                }
            }
        };
    }

    public fun multiple_nested_blocks(x: u64): u64 {
        {
            {
                {
                    {
                        // 4th level of nesting
                    }
                }
            }
        };
        // Another set of nested blocks
        {
            {
                {
                    {
                        // Another 4th level of nesting
                    }
                }
            }
        };
        x
    }
}

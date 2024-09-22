module 0x42::excessive_nesting_false_negative {
    public fun complex_logic_without_blocks(x: u64) {
        if x > 10 if x < 20 if x != 15 if x % 2 == 0 { x = x + 1 } else { x = x - 1 } else { x = x * 2 } else { x = x / 2 } else { x = 0 };
        // This is essentially 5 levels of nesting, but without explicit block structures
    }

    public fun mixed_control_structures(x: u64) {
        while (x > 0) {
            if (x % 2 == 0) {
                for (i in 0..x) {
                    if (i % 3 == 0) {
                        // This is 4 levels deep, mixing different control structures
                        // which might be missed by a lint focusing only on block structures
                    }
                }
            }
            x = x - 1;
        }
    }
}

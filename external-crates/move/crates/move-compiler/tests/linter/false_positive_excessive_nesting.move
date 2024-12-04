module 0x42::reasonable_nesting {
    fun match_nested(x: u64): u64 {
        if (x > 0) {
            if (x > 10) {
                if (x > 20) {
                    if (x > 30) { // Complex but necessary business logic
                        return 1
                    }
                }
            }
        };
        0
    }
}

module 0x42::M {

    fun func1(x: u64) {
        // Consecutive ifs with different conditions (should not trigger lint)
        if (x < 5) {
            // Some logic here
        };
        if (x >= 5) {
            // Some other logic here
        };
    }
}

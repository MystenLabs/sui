module 0x42::M {

    fun func1(x: u64) {
      // Consecutive ifs with the same condition (should trigger lint)
        if (x > 10) {
            // Some logic here
        };
        if (x > 10) {
            // Some other logic here
        };

        // Consecutive ifs with the same condition in different scope (should trigger lint)
        if (x == 3) {
            // Some logic here
        };
        // Some unrelated code
        if (x == 3) {
            // Some other logic here
        };
    }
}

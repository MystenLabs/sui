module 0x42::loop_test {
    // This function should trigger the linter
    public fun true_positive_infinite_loop() {
        while (true) {
            // This should trigger the linter
        }
    }

    public fun true_positive_finite_loop() {
        let i = 0;
        while (true) {
            if (i == 10) break;
            i = i + 1;
        }
    }
}

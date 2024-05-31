module 0x42::loop_test {

    // True Negative Cases
    // These should not trigger the linter warning
    public fun true_negative_correct_infinite_loop() {
        loop {
            // This is the correct way to write an infinite loop
        }
    }

    public fun true_negative_while_with_condition(n: u64) {
        let i = 0;
        while (i < n) {
            i = i + 1;
        }
    }
}

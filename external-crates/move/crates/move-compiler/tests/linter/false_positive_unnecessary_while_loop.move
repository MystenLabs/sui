module 0x42::loop_test {

    public fun false_positive_complex_condition() {
        while (complex_always_true_condition()) {
            // This might trigger the linter if the condition is too complex to analyze
        }
    }

   fun complex_always_true_condition(): bool {
        true
    }
}

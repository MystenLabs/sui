module 0x42::loop_test {

    public fun false_negative_obfuscated_true() {
        let always_true = true;
        while (always_true) {
            // This should trigger the linter but might not due to indirection
        }
    }
}

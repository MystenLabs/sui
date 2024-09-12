module 0x42::empty_loop_lint_test {
    // True Negative Cases
    public fun true_negative_while_with_content() {
        let i = 0;
        while (i < 10) {
            i = i + 1;
        }
    }

    public fun true_negative_loop_with_break() {
        let i = 0;
        loop {
            if (i >= 10) break;
            i = i + 1;
        }
    }
}

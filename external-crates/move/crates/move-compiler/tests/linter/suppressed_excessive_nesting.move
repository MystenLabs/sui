module 0x42::excessive_nesting {

    #[allow(lint(excessive_nesting), unused_assignment)]
    fun intentionally_nested(x: u64) {
        if (x > 0) {
            if (x > 10) {
                if (x > 20) {
                    if (x > 30) { // Suppressed with attribute
                        x = x + 1;
                    }
                }
            }
        }
    }
}

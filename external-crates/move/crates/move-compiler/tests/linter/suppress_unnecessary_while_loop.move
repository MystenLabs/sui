module 0x42::loop_test {

    #[allow(lint(while_true_to_loop))]
    public fun suppressed_while_true() {
        while (true) {
            // This loop will run forever, but won't trigger the linter warning
            if (false) {
                break
            }
        }
    }
}

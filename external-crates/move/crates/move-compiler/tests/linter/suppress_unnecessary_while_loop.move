module 0x42::loop_test {

    #[allow(lint(while_true))]
    public fun suppressed_while_true() {
        while (true) {};
    }
}

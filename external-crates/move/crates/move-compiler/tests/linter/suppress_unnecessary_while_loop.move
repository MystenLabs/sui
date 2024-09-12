module 0x42::loop_test {

    #[allow(lint(while_true))]
    public fun suppressed_while_true() {
        let i = 0;
        while(true) {
            if (i >= 10) break;
            i = i + 1;
        }
    }
}

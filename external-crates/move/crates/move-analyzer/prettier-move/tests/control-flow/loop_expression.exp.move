module test::loop_expression {
    public fun test_loop() {
        loop break;

        loop {
            break;
        };

        loop {
            break;
        };
    }
}

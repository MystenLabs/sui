module 0x42::loop_test {
    public fun true_positive_infinite_loop() {
        while (true) {};
        while (true) { break }
    }
}

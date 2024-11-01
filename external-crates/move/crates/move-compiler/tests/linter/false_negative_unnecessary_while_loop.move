module 0x42::loop_test {

    // These should trigger but currently dont
    public fun false_negative_obfuscated_true() {
        let always_true = true;
        while (always_true) {};
        while (true && true) {};
        while (true || false) {};
        while (1 > 0) {};
    }
}

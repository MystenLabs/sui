module 0x42::loop_test {

    public fun true_negative_while_with_condition() {
        let b = false;
        while (false) {};
        while (b) {};
        while (false && true) {};
        while (false || false) {};
        while (0 > 1) {};
    }
}

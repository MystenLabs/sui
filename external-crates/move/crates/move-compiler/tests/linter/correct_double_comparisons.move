module 0x42::M {

    fun func1(x: u64) {
        if (x < 5 || x > 10) {
        };

        if (x < 5 && x > 10) {
        };
    }
}

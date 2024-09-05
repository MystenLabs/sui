module 0x1::bench {
    fun check(check: bool, code: u64) {
        if (check) () else abort code
    }

    public fun bench() {
        let mut i = 0;
        // 10000 is the number of loops to make the benchmark run for a couple of minutes,
        // which is an eternity.
        // Adjust according to your needs, it's just a reference
        while (i < 10000) {
            1;
            10 + 3;
            10;
            7 + 5;
            let x = 1;
            let y = x + 3;
            check(x + y == 5, 10);
            i = i + 1;
        };
    }
}

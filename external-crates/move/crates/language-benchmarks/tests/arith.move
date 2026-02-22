module 0x1::bench {
    const COUNT: u64 = 10_000u64;
    const MAX_U64: u64 = 18446744073709551615;

    fun check(check: bool, code: u64) {
        if (check) () else abort code
    }

    public fun bench_add() {
        let mut sum = 0;
        let mut i = 0;
        while (i < COUNT) {
            sum = sum + i;
            i = i + 1;
        }
    }

    public fun bench_sub() {
        let mut sum = COUNT * COUNT;
        let mut i = COUNT;
        while (i > 0) {
            sum = sum - i;
            i = i - 1;
        }
    }

    public fun bench_mul() {
        let mut sum = 1;
        let mut i = 1;
        while (i < COUNT) {
            sum = sum + (i * i);
            i = i + 1;
        }
    }

    public fun bench_div() {
        let mut sum = MAX_U64 / 2;
        let mut i = 3;
        while (i < COUNT) {
            sum = sum / i + sum / 100;
            i = i + 1;
        }
    }

    public fun bench_mod() {
        let mut sum = MAX_U64 / 2;
        let mut i = 2;
        while (i < COUNT) {
            sum = sum % i + sum;
            i = i + 1;
        }
    }

    public fun bench_loop_bounce_arith() {
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
        }
    }

}

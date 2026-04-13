module 0x2::bench {
    const COUNT: u64 = 10_000u64;
    const MAX_U64: u64 = 18446744073709551615;

    public fun bench_add() {
        let mut sum = 0;
        let mut i:u64 = 0;
        while (i < COUNT) {
            sum = sum + i;
            i = i + 1;
        }
    }

    public fun bench_sub() {
        let mut sum = COUNT * COUNT;
        let mut i:u64 = COUNT;
        while (i > 0) {
            sum = sum - i;
            i = i - 1;
        }
    }

    public fun bench_mul() {
        let mut sum = 1;
        let mut i:u64 = 1;
        while (i < COUNT) {
            sum = sum + (i * i);
            i = i + 1;
        }
    }

    public fun bench_div() {
        let mut sum = MAX_U64 / 2;
        let mut i:u64 = 3;
        while (i < COUNT) {
            sum = sum / i + sum / 100;
            i = i + 1;
        }
    }

    public fun bench_mod() {
        let mut sum = MAX_U64 / 2;
        let mut i:u64 = 2;
        while (i < COUNT) {
            sum = sum % i + sum;
            i = i + 1;
        }
    }

    public fun bench_loop_bounce_arith() {
        let mut i:u64 = 0;
        // 10000 is the number of loops to make the benchmark run for a couple of minutes,
        // which is an eternity.
        // Adjust according to your needs, it's just a reference
        while (i < 10000) {
            1u64;
            10u64 + 3u64;
            10u64;
            7u64 + 5u64;
            let x = 1u64;
            let y = x + 3u64;
            assert!(x + y == 5u64);
            i = i + 1;
        }
    }

}

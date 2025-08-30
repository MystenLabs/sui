module 0x1::bench {
    const COUNT: u64 = 10_000u64;
    public fun bench_while_loop(): u64 {
        let mut sum = 0;
        let mut i = 0;
        while (i < COUNT) {
            sum = sum + i;
            i = i + 1;
        };
        sum
    }
    public fun bench_loop_loop(): u64 {
        let mut sum = 0;
        let mut i = 0;
        loop {
            sum = sum + i;
            i = i + 1;
            if (i >= COUNT) {
                break
            };
        };
        sum
    }
}
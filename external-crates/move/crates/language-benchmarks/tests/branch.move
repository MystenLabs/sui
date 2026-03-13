module 0x1::bench {
    const COUNT: u64 = 10_000u64;

    public fun bench_branch() {
        let mut sum = 0;
        let mut i = 0;
        while (i < COUNT) {
            let rem = i % 7;
            if (rem == 0) {
                sum = sum + 100;
            };
            if (rem == 1) {
                sum = sum + 7;
            };
            if (rem == 2) {
                sum = sum + i + 8;
            };
            if (rem == 3) {
                sum = sum + 1;
            };
            if (rem == 4) {
                sum = sum + i + 2;
            };
            if (rem == 5) {
                sum = sum + 6;
            };
            if (rem == 6) {
                sum = sum + i + 7;
            };
            i = i + 1;
        }
    }
}
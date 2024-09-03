module 0x1::bench {
    const COUNT: u64 = 10_000u64;

    public fun bench() {
        let mut sum = 0;
        let mut i = 0;
        while (i < COUNT) {
            sum = sum + i;
            i = i + 1;
        }
    }
}

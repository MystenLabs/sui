module 0x1::bench {
    const COUNT: u64 = 100_000u64;

    public fun bench() {
        let mut v = vector::empty<u64>();
        let mut i = 0;
        while (i < COUNT) {
            v.push_back(i);
            i = i + 1;
        };
        i = 0;
        while (i < COUNT) {
            v.pop_back();
            i = i + 1;
        };
    }
}

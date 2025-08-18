module 0x1::bench {
    const COUNT: u64 = 100_000u64;

    public fun bench_vector_push_pop() {
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

    public fun bench_vector_append() {
        let mut v1 = vector::empty<u64>();
        let mut v2 = vector::empty<u64>();
        let mut i = 0;
        while (i < COUNT) {
            v1.push_back(i);
            v2.push_back(i);
            i = i + 1;
        };
        vector::append<u64>(&mut v1, v2);
        // assert!(vector::length(&v1) == COUNT * 2);
    }
}

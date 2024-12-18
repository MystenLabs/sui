module 0x1::bench {

    const COUNT: u64 = 10_000u64;

    public fun bench(): u64 {
        let mut i = 0;
        while (i < COUNT) {
            i = i + call_empty_function();
        };
        i
    }

    public fun call_empty_function(): u64 {
        1
    }
}

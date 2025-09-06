module 0x1::bench {
    const COUNT: u64 = 10_000u64;

    //
    // Global helpers
    //
    fun check(check: bool, code: u64) {
        if (check) () else abort code
    }

    fun empty_function(): u64 {
        1
    }

    public fun bench_call_empty_function(): u64 {
        let mut i = 0;
        while (i < COUNT) {
            i = i + empty_function();
        };
        i
    }

    public fun bench_call() {
        let mut i = 0;
        // 3000 is the number of loops to make the benchmark run for a couple of minutes,
        // which is an eternity.
        // Adjust according to your needs, it's just a reference
        while (i < 3000) {
            let b = call_1(@0x0, 128);
            call_2(b);
            i = i + 1;
        };
    }

    // use 0x1::bench_xmodule_call;

    public fun bench_xmodule_call() {
        0x1::bench_xmodule_call::bench_call();
    }

    fun call_1(addr: address, val: u64): bool {
        let b = call_1_1(&addr);
        call_1_2(val, val);
        b
    }

    fun call_1_1(_addr: &address): bool {
        true
    }

    fun call_1_2(val1: u64, val2: u64): bool {
        val1 == val2
    }

    fun call_2(b: bool) {
        call_2_1(b);
        check(call_2_2() == 400, 200);
    }

    fun call_2_1(b: bool) {
        check(b == b, 100)
    }

    fun call_2_2(): u64 {
        100 + 300
    }
}

module 0x1::bench_xmodule_call {
    fun check(check: bool, code: u64) {
        if (check) () else abort code
    }

    public fun bench_call() {
        let mut i = 0;
        // 3000 is the number of loops to make the benchmark run for a couple of minutes,
        // which is an eternity.
        // Adjust according to your needs, it's just a reference
        while (i < 3000) {
            let b = call_1(@0x0, 128);
            call_2(b);
            i = i + 1;
        };
    }

    fun call_1(addr: address, val: u64): bool {
        let b = call_1_1(&addr);
        call_1_2(val, val);
        b
    }

    fun call_1_1(_addr: &address): bool {
        true
    }

    fun call_1_2(val1: u64, val2: u64): bool {
        val1 == val2
    }

    fun call_2(b: bool) {
        call_2_1(b);
        check(call_2_2() == 400, 200);
    }

    fun call_2_1(b: bool) {
        check(b == b, 100)
    }

    fun call_2_2(): u64 {
        100 + 300
    }
}
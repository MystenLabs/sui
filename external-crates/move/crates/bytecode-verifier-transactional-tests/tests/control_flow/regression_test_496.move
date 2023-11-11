//# publish
module 0x42::test_case {
    public fun test_loop(a: u64): u64 {
        if (a == 2) {
            1
        } else if (a == 3) {
            let i = 0;
            let ret = 0;
            while (i < a) {
                ret = ret + i;
                i = i + 1;
            };
            ret
        } else {
            abort 10000
        }
    }
}

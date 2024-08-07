module 0x42::TestLiveVars {
    public struct R has copy, drop {
        x: u64
    }

    fun test1(r_ref: &R) : u64 {
        let x_ref = & r_ref.x;
        *x_ref
    }

    fun test2(b: bool) : u64 {
        let r1 = R {x: 3};
        let r2 = R {x: 4};
        let mut r_ref = &r1;
        if (b) {
            r_ref = &r2;
        };
        test1(r_ref)
    }

    fun test3(mut n: u64, mut r_ref: &R) : u64 {
        let r1 = R {x: 3};
        let r2 = R {x: 4};
        while (0 < n) {
            if (n/2 == 0) {
                r_ref = &r1
            } else {
                r_ref = &r2;
            };
            n = n - 1
        };
        test1(r_ref)
    }
}

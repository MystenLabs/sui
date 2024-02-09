module 0x42::m {
    fun t0(cond: bool): u64 {
        'name: {
            if (cond) { break 'name 10 };
            if (cond) { continue 'name };
            20
        }
    }

    fun t1(cond: bool): u64 {
        'name: loop {
            if (cond) { return 'name 10 };
        }
    }

    fun t2(cond: bool): u64 {
        'outer: loop {
            'inner: loop {
                if (cond) { return 'outer 10 };
                if (cond) { return 'inner 20 };
            };
        }
    }

    fun t3(cond: bool) {
        'outer: while (cond) {
            'inner: while (cond) {
                if (cond) { return 'outer };
                if (cond) { return 'inner };
            }
        }
    }

    fun t4(cond: bool) {
        'outer: while (cond) {
            let _x = 'inner: {
                if (cond) { return 'outer };
                if (cond) { break 'inner 10 };
                20
            };
        }
    }

    fun t5() {
        'l: loop {
            'l: loop {
                return 'l
            }
        }
    }

    fun t6(cond: bool): u64 {
        'name: {
            if (cond) { return 'name2 10 };
            20
        }
    }

    fun t7(cond: bool): u64 {
        'name: loop {
            if (cond) { continue 'name2 };
            if (cond) { break 'name2 10 };
        }
    }

    fun t8(cond: bool): u64 {
        'outer2: loop {
            'inner2: loop {
                if (cond) { break 'outer 10 };
                if (cond) { break 'inner 20 };
            };
        }
    }

    fun t9(cond: bool) {
        'outer: while (cond) {
            'inner: while (cond) {
                if (cond) { continue 'outer2 };
                if (cond) { break 'inner2 };
            }
        }
    }

    fun t10() {
        'l: loop {
            'l: loop {
                break 'l2
            }
        }
    }
}

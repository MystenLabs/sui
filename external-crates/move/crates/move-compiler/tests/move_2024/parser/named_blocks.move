module 0x42::m {

    fun t0(cond: bool): u64 {
        'name: {
            if (cond) { return 'name 10 };
            20
        }
    }

    fun t1(cond: bool): u64 {
        'name: loop {
            if (cond) { break 'name 10 };
        }
    }
    fun t1_inline(cond: bool): u64 {
        'name: loop if (cond) { break 'name 10 }
    }

    fun t2(cond: bool): u64 {
        'outer: loop {
            'inner: loop {
                if (cond) { break 'outer 10 };
                if (cond) { break 'inner 20 };
            };
        }
    }

    fun t3(cond: bool) {
        'outer: while (cond) {
            'inner: while (cond) {
                if (cond) { break 'outer };
                if (cond) { break 'inner };
            }
        }
    }
    fun t3_inline(cond: bool) {
        'outer: while (cond)
            'inner: while (cond) {
                if (cond) { break 'outer };
                if (cond) { break 'inner };
            }
    }

    fun t4(cond: bool) {
        'outer: while (cond) {
            let _x = 'inner: {
                if (cond) { break 'outer };
                if (cond) { return 'inner 10 };
                20
            };
        }
    }

    fun t5() {
        'l: loop {
            'l: loop {
                break 'l
            }
        }
    }

}

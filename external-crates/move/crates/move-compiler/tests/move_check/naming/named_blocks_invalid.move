module 0x42::m {
    fun t0(_cond: bool) {
        'name: {
            if (cond) { break 'name 10 };
            if (cond) { continue 'name };
            20
        }
    }

    fun t1(_cond: bool) {
        loop 'name: {
            if (cond) { return 'name 10 };
        }
    }

    fun t2(_cond: bool) {
        loop 'outer: {
            loop 'inner: {
                if (cond) { return 'outer 10 };
                if (cond) { return 'inner 20 };
            };
        }
    }

    fun t3(_cond: bool) {
        while (cond) 'outer: {
            while (cond) 'inner: {
                if (cond) { return 'outer };
                if (cond) { return 'inner };
            }
        }
    }

    fun t4(_cond: bool) {
        while (cond) 'outer: {
            let _x = 'inner: {
                if (cond) { return 'outer };
                if (cond) { break 'inner 10 };
                20
            };
        }
    }

    fun t5() {
        loop 'l: {
            loop 'l: {
                return 'l
            }
        }
    }

    fun t6(_cond: bool) {
        'name: {
            if (cond) { return 'name2 10 };
            20
        }
    }

    fun t7(_cond: bool) {
        loop 'name: {
            if (cond) { continue 'name2 };
            if (cond) { break 'name2 10 };
        }
    }

    fun t8(_cond: bool) {
        loop 'outer2: {
            loop 'inner2: {
                if (cond) { break 'outer 10 };
                if (cond) { break 'inner 20 };
            };
        }
    }

    fun t9(_cond: bool) {
        while (cond) 'outer: {
            while (cond) 'inner: {
                if (cond) { continue 'outer2 };
                if (cond) { break 'inner2 };
            }
        }
    }

    fun t10() {
        loop 'l: {
            loop 'l: {
                break 'l2
            }
        }
    }
}

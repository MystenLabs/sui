module 0x42::m {

    fun t0(_cond: bool) {
        'name: {
            if (cond) { return 'name 10 };
            20
        }
    }

    fun t1(_cond: bool) {
        loop 'name: {
            if (cond) { break 'name 10 };
        }
    }

    fun t2(_cond: bool) {
        loop 'outer: {
            loop 'inner: {
                if (cond) { break 'outer 10 };
                if (cond) { break 'inner 20 };
            };
        }
    }

    fun t3(_cond: bool) {
        while (cond) 'outer: {
            while (cond) 'inner: {
                if (cond) { break 'outer };
                if (cond) { break 'inner };
            }
        }
    }

    fun t4(_cond: bool) {
        while (cond) 'outer: {
            let _x = 'inner: {
                if (cond) { break 'outer };
                if (cond) { return 'inner 10 };
                20
            };
        }
    }

    fun t5() {
        loop 'l: {
            loop 'l: {
                break 'l
            }
        }
    }

}

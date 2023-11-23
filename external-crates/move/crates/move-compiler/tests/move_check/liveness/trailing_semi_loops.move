module 0x42::m {
    fun t0() {
        loop ();
    }

    fun t1() {
        { (loop (): ()) };
    }

    fun t2() {
        loop {
            let x = 0;
            0 + x + 0;
        };
    }

    fun t3() {
        loop {
            // TODO can probably improve this message,
            // but its different than the normal trailing case
            let _: u64 = if (true) break else break;
        }
    }

    fun t4() {
        loop {
            break;
        }
    }

    fun t5(cond: bool) {
        loop {
            if (cond) {
                break;
            } else {
                ()
            }
        }
    }

    fun t6(cond: bool) {
        loop {
            if (cond) continue else break;
        }
    }

    fun t7(cond: bool) {
        loop {
            if (cond) abort 0 else return;
        }
    }

    fun t8(cond: bool) {
        let x;
        loop {
            if (cond) {
                x = 1;
                break
            } else {
                x = 2;
                continue
            };
        };
        x;
    }

    fun t9(cond: bool) {
        loop {
            if (cond) {
                break;
            } else {
                continue;
            };
        }
    }

    fun t10(cond: bool) {
        loop {
            if (cond) {
                return;
            } else {
                abort 0;
            };
        }
    }
}

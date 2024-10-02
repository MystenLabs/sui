module 0x42::m {
    fun t0(): bool {
        loop { break 0 };
    }

    fun t1(): u64 {
        loop { break true } 
    }

    fun t2(cond: bool): bool {
        if (cond) {
            loop { break 0 }
        } else {
            loop { break false }
        }
    }

    fun t3(cond: bool): bool {
        while (cond) { break true } 
    }

    fun t4(cond: bool): bool {
        while (cond) { break true }; 
    }

    fun t5(cond: bool): u64 {
        let x = 0;
        loop { 
            if (cond) {
               break true
            } else {
                x = x + 1;
            }
        } 
    }

    fun t6(cond: bool) {
        let x = while (cond) { };
        x
    }

    fun t7(): u64 {
        let x = 0;
        if (loop { 
            if (x == 10) {
                break true
            } else {
                x = x + 1;
            }
        } == 0) {
            x
        } else {
            0
        }
    }

    fun t8(): u64 {
        loop {
            break 15
        };
        10
    }

    struct R {f: u64}

    fun t9(): u64 {
        loop {
            break R { f: 0 }
        };
        10
    }
}

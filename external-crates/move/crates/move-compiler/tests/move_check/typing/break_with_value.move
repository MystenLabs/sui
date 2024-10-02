module 0x42::m {
    fun t0() {
        let _x = loop { break 0 };
    }

    fun t1(): u64 {
        loop { break 0 } 
    }

    fun t2(cond: bool): bool {
        if (cond) {
            loop { break true }
        } else {
            loop { break false }
        }
    }

    fun t3(cond: bool): bool {
        loop { 
            break if (cond) {
                loop { break true }
            } else {
                loop { break false }
            }
        } 
    }

    fun t4(cond: bool): u64 {
        let x = 0;
        loop { 
            if (cond) {
                break x  
            } else {
                x = x + 1;
            }
        } 
    }

    fun t5(): u64 {
        let x = 0;
        if (loop { 
            if (x > 10) {
                break x
            } else {
                x = x + 1;
            }
        } == 0) {
            x
        } else {
            0
        }
    } 

    fun t6(): bool {
        loop {
            break loop {
                break true
            }
        }
    }

    struct R {f: u64}

    fun t7(): u64 {
        let R { f } = loop {
            break R { f: 0 }
        };
        f
    }
}

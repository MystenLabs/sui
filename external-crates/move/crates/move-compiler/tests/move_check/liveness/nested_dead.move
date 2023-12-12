module 0x8675309::m {
    fun t(): u64 {
        return return 0 
    }

    fun t2() {
        if (true) { return abort 0 } else { abort 0 };
        return
    }

    fun t3() {
        loop { return abort 0 }
    }

    fun t4() {
        if (true) { return abort 0 } else { return abort 0 } 
    }

    fun t5(): u64 {
        let x = if (return 1) { 0 } else { 1 };
        x
    }

    fun t6(): u64 {
        while (return 0) {
            break 
        };
        10
    }
}

module 0x8675309::M {
    fun t0() {
        let x = 0u64;
        copy x;
    }

    fun t1() {
        let x = 0u64;
        let x = copy x;
        x;
    }

    fun t2(cond: bool) {
        if (cond) {
            let x = 0u64;
            copy x;
        }
    }

    fun t3(cond: bool) {
        let x = 0u64;
        copy x;
        if (cond) {
            copy x;
        }
    }

    fun t4(cond: bool) {
        let x = 0u64;
        if (cond) {
            copy x;
        } else {
            copy x;
        }
    }

    fun t5(cond: bool) {
        let x = 0u64;
        while (cond) {
            copy x;
        };
    }

    fun t6(cond: bool) {
        let x = 0u64;
        while (cond) {
            copy x;
        };
        copy x;
    }

}

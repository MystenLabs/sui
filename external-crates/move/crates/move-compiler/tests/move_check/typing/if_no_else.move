
module a::m {
    fun t(cond: bool) {
        if (cond) 0u64;
        if (cond) foo();
        if (cond) {
            let x = 0;
            let y = 1u64;
            x * y
        }
    }

    fun foo(): u64 { 0 }
}

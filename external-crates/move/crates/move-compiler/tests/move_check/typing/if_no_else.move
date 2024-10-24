
module a::m {
    fun t(cond: bool) {
        if (cond) 0;
        if (cond) foo();
        if (cond) {
            let x = 0;
            let y = 1;
            x * y
        }
    }

    fun foo(): u64 { 0 }
}

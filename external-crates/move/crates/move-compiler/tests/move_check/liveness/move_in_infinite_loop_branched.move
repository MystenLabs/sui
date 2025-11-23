module 0x42::m {
    fun main(cond: bool) {
        let x = 0u64;
        if (cond) {
            loop { let y = move x; y / y; }
        } else {
            loop { let y = move x; y % y; }
        }
    }
}

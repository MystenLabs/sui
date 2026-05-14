module 0x42::m {

    fun match_literal(x: u64): u64 {
        let 0u64 = x else { return 1 };
        0
    }

}

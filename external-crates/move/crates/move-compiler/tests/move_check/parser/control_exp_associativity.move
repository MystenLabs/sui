// tests that control structures are right associative when not immediately followed by a block

module 0x42::M {
    fun t(cond: bool) {
        let _: u64 = 1 + if (cond) 0 else 10 + 10;
        let _: bool = true || if (cond) false else 10 == 10;
        let _: bool = if (cond) 10 else { 10 } == 10;
        let _: u64 = if (cond) 0 else { 10 } + 1;
    }
}

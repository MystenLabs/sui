module a::m {
    fun t() {
        let _: u64 = 'a: loop { break 'a 0 } + 1;
        let _: u64 = 'a: loop { loop { break 'a 0 } } + 1;
        let _: u64 = loop 'a: { break 0 } + 1;
    }
}

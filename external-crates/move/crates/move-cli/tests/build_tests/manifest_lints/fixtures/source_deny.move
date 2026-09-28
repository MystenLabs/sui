module test::m {
    #[deny(unused_variable)]
    public fun warning() {
        let unused = 0u64;
    }
}

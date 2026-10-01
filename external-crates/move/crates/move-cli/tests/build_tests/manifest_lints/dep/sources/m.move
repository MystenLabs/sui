#[warn(lint(all))]
module dep::m {
    #[deny(unused_variable)]
    public fun warning() {
        let unused = 0u64;
        abort 1
    }
}

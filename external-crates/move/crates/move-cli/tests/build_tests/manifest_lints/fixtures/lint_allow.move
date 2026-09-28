module test::m {
    #[allow(lint(abort_without_constant))]
    public fun lint() {
        abort 1
    }
}

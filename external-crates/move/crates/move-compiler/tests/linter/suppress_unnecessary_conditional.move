module a::m {

    #[allow(lint(unnecessary_conditional))]
    public fun t0(condition: bool) {
        if (condition) true else false;
    }

    #[allow(lint(unnecessary_conditional))]
    public fun t1(condition: bool) {
        if (condition) vector<u8>[] else vector[];
    }

    #[allow(lint(unnecessary_conditional))]
    public fun t2(condition: bool) {
        if (condition) @0 else @0;
    }
}

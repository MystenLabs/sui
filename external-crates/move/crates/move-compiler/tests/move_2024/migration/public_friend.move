module a::m {

    public( friend ) fun t0() {}

    public(friend) fun t1() {}

    public(
        friend) fun t2() {}

    public(
        friend
        ) fun t3() {}

    public(
        friend
        /* comment */
    ) fun t4() {}

    /* stays-comment */
    public(
        friend
        /* deleted-comment */
    )/* stays-comment */ fun t5() {}

    /*stays*/public(/*deleted*/friend/*deleted*/)/*stays*/fun t6() {}
}

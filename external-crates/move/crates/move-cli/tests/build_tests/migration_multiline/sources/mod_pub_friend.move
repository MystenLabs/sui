module A::x {
    public(friend) fun t1() {}

    public(
        friend) fun t2() {}

    public(
        friend
        ) fun t3() {}

    public(
        friend /* deleted */
    ) fun t4() {}

    /* stays */ public(
        /* deleted */
        friend
        /*deleted */) /* stays */ fun t5() {} // stays
}

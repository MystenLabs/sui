module a::m {

    public( friend ) fun t0() {}
    public(friend) fun t1() {}
    /*stays*/public(/*deleted*/friend/*deleted*/)/*stays*/fun t2() {}

}

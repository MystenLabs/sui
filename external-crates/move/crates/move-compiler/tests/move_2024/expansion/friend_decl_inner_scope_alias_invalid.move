address 0x42 {
module Q {
    public fun q() {}
}

module R {
    friend Q;

    public(friend) fun r() {
        use 0x42::Q;
        Q::q()
    }
}
}

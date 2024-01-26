address 0x42 {
module Q {
    use 0x42::R;
    friend R;
    public(friend) fun q() {}
}

module R {
    public fun r() {
        use 0x42::Q;
        Q::q()
    }
}
}

module a::acquires {
    struct S has drop { acquires: u64 }
    fun acquires<acquires>(acquires: acquires): acquires {
        acquires
    }
    fun call(acquires: S): u64 {
        let S { acquires } = acquires<S>(acquires);
        acquires
    }
}

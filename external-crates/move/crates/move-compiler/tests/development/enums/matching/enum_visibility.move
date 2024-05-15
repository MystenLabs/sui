module 0x42::m {

    public enum E {
        A(u64),
    }

}

module 0x42::n {

    use 0x42::m;

    fun test(e: m::E): u64 {
        match (e) {
            m::E::A(t) => t,
        }
    }

}

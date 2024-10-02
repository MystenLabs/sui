module 0x42::m {

    public struct S { t: u64 }

}

module 0x42::n {

    use 0x42::m;

    fun test(s: m::S): u64 {
        match (s) {
            m::S { t } => t,
        }
    }

}

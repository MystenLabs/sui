module 0x042::m {

    const Z8: u8 = 0;

    public struct S has drop { n: u64 }

    fun test(s: S): u64 {
        match (s) {
            S { n: Z8 } => n,
            _ => 10,
        }
    }

}

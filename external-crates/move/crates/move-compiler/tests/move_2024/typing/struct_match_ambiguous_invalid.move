module 0x42::m {
    public struct S<phantom T> { x: u64 } has drop;

    fun t(): u64{
        let s = S { x: 0 };
        match (s) {
            _ => 10
        }
    }
}

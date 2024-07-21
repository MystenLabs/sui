// Testcase 1: No If-Else
module 0x12::tc1 {
    public fun foo(x: u64): u64 {
        let sum = x + 1;
        let abc = x + 2;
        let xyz = 1;
        return sum + abc + xyz
    }
}

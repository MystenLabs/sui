// Testcase 2: Only If
module 0x12::tc2 {
    public fun foo(x: u64): u64 {
        let sum = x + 1;
        let abc = x + 2;
        let xyz = 1;
        if (sum == 2) {
            let a = 2;
            sum = sum - a;
        };
        let c = 11;
        sum = sum + c;
        return sum
    }
}

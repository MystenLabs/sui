// Testcase 3: Normal If-Else
module 0x12::tc3 {
    public fun foo(x: u64): u64 {
        let sum = x + 1;
        let abc = x + 2;
        let xyz = 1;
        sum = sum + if (sum == 2) {
            let a = 2;
            a - sum
        } else {
            let a = 5;
            sum - a
        };
        let c = 11;
        return sum + c
    }
}

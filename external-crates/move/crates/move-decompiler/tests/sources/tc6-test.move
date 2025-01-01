// Testcase 6: Has `return` in if-body and else-body
module 0x12::tc6 {
    public fun foo(x: u64): u64 {
        let sum = x + 1;
        let abc = x + 2;
        let xyz = 1;
        if (sum == 2) {
            let a = 2;
            return a-sum
        } else {
            let a = 5;
            return sum-a
        };
        let c = 11;
        return c - sum
    }
}

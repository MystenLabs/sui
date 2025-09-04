// Testcase 4: Has `return` in if-body
module 0x12::tc4 {
    public fun foo(x: u64): u64 {
        let sum = x + 1;
        let abc = x + 2;
        let xyz = 1;
        if (sum == 2) {
            let a = 2;
            return sum - a
        } else {
            let a = 5;
            sum = sum + a;
        };
        let c = 11;
        return sum - c
    }
}

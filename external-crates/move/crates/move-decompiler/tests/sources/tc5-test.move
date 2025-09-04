// Testcase 5: Has `return` in else-body
module 0x12::tc5 {
    public fun foo(x: u64): u64 {
        let sum = x + 1;
        let abc = x + 2;
        let xyz = 1;
        if (sum == 2) {
            let a = 2;
            sum = sum * a;
        } else {
            let a = 5;
            return a - sum
        };
        let c = 11;
        return c - sum + abc - xyz
    }
}

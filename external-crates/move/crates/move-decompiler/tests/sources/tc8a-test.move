// Testcase 8: `while`
module 0x12::tc8a {
    public fun foo() : u64 {
        let a = 0;
        let i = a;
        while (i < 5) {
            i = i + 1;
        };
        i = i + 2;
        i
    }
}

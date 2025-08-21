// Testcase 10: `while` inside conditional statement
module 0x12::tc10 {
    public fun foo(): u64 {
        let i = 0;
        let x = i + 1;
        i = i + if (x > 1) {
            x = x + 1;
            while (x < 5) {
                x = x + 1;
            };
            x = x - 1;
            x
        } else {
            x = x + 2;
            while (x < 6) {
                x = x + 1;
            };
            x = x - 2;
            x
        };
        let j = 99;
        return i + j
    }
}

// Testcase 11: nested `while`
module 0x12::tc11 {
    public fun foo(): u64 {
        let i = 0;
        let x = i + 1;
        while (x < 5) {
            let y = x + 1;
            while (y >= 0) {
                let z = y + 2;
                y = y + 1;
                while (z != 7) {
                    z = z + 1;
                    y = y - z;
                };
                y = y + 3 - z;
            };
            x = x + y;
        };
        let j = 99;
        return i+x+j
    }
}

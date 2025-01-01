// Testcase 9: `loop`
module 0x12::tc9 {
    public fun foo(): u64 {
        let i = 0;
        loop {
            i = i + 1;
            if (i / 2 == 0) continue;
            if (i == 5) break;
            let a = 69;
            a = a + i;
            i = i + a;
         };
        let j = 99;
        i + j
    }
}

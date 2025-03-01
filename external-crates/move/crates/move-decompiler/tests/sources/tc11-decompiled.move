module 0x12::tc11 {
    public fun foo() : u64 {
        let v0 = 0;
        let v1 = v0 + 1;
        while (v1 < 5) {
            let v2 = v1 + 1;
            while (v2 >= 0) {
                let v3 = v2 + 2;
                v2 = v2 + 1;
                while (v3 != 7) {
                    let v4 = v3 + 1;
                    v3 = v4;
                    v2 = v2 - v4;
                };
                let v5 = v2 + 3;
                v2 = v5 - v3;
            };
            v1 = v1 + v2;
        };
        v0 + v1 + 99
    }

    // decompiled from Move bytecode v6
}

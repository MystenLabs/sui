module 0x12::tc7 {
    public fun foo(arg0: u64) : u64 {
        let v0 = arg0 + 1;
        let v1 = v0;
        let v2 = arg0 + 2;
        let v3 = v2;
        let v4 = 1;
        let v5 = v4;
        if (v0 == 2) {
            let v6 = arg0 + 2;
            let v7 = v6;
            if (v6 > 3) {
                let v8 = arg0 + 3;
                if (v8 > 10) {
                    v7 = v6 + 4 - v8;
                } else {
                    v7 = v6 - 6 - v8;
                };
                v5 = v0 + v8 + 1;
            } else {
                v1 = v4 - 5;
            };
            v3 = v2 + 7 - v7;
        } else {
            if (arg0 + 5 < 3) {
                let v9 = arg0 + 3;
                if (v9 < 10) {
                    v5 = v2 + 4;
                } else {
                    v3 = v4 + 6;
                };
                v1 = v0 + v9 + 2;
            };
            v1 = arg0 + 1 - v1;
        };
        v5 + v3 + v1 + 11
    }

    // decompiled from Move bytecode v6
}

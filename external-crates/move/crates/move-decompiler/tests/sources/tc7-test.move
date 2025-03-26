// Testcase 7: Has multilevel if-else
module 0x12::tc7 {
    public fun foo(x: u64): u64 {
        let sum = x + 1;
        let abc = x + 2;
        let xyz = 1;
        if (sum == 2) {
            let a = x + 2;
            if (a > 3) {
                let c = x + 3;
                if (c > 10) {
                    let k = 4;
                    a = a + k - c;
                } else {
                    let k = 6;
                    a = a - k - c;
                };
                c = c + 1;
                xyz = sum + c;
            } else {
                let c = 5;
                sum = xyz - c;
            };
            let b = 7;
            abc = abc + b - a;
        } else {
            let a = x + 5;
            if (a < 3) {
                let c = x + 3;
                if (c < 10) {
                    let k = 4;
                    xyz = abc + k
                } else {
                    let k = 6;
                    abc = xyz + k
                };
                c = c + 2;
                sum = sum + c;
            };
            let c = x + 1;
            sum = c - sum
        };
        let c = 11;
        return xyz + abc + sum + c
    }
}

//# init --edition 2024.alpha

//# publish
module 0x42::m {
    public struct T has drop {
        x: u64,
        y: u64
    }

    public struct S has drop {
        a: u64,
        b: T,
    }

    public fun make_s(a: u64, x: u64, y: u64): S{
        S {
            a,
            b: T { x, y }
        }
    }

    public fun t(s: S): u64 {
        let s0 = &s;
        let b0 = &s0.b;
        let a = s0.a;
        let x = b0.x;
        let s1 = &s;
        let b1 = &s1.b;
        let n = x + a;
        let y = b1.y;
        let m = y + n;
        let m = m + m;
        m
    }
}


//# run
module 0x42::main {
    fun main() {
        let s = 0x42::m::make_s(1, 2, 3);
        let value = 0x42::m::t(s);
        let s = 0x42::m::make_s(1, 2, 3);
        assert!(0x42::m::t(s) == 12, value);
    }
}


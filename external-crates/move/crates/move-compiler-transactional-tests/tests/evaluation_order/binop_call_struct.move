//# publish
module 0x42::m {
    struct S has drop {
        x: u64,
        y: u64,
        z: u64,
    }

    fun inc_x(self: &mut S) {
        self.x = self.x + 1;
    }

    public fun test1(): u64 {
        let s = S { x: 1, y: 2, z: 3 };
        s.x + {inc_x(&mut s); s.x} + {inc_x(&mut s); s.x}
    }

    fun inc_xx(self: &mut S, by: u64) {
        self.x = self.x + by;
    }

    public fun test2(): u64 {
        let s = S { x: 1, y: 2, z: 3 };
        s.x + {inc_xx(&mut s, 3); s.x} + {inc_xx(&mut s, 11); s.x}
    }

    public fun test3(): u64 {
        let x = 1;
        let S {y, x, z} = S { x, y: {x = x + 1; x}, z: {x = x + 1; x} };
        x + y + z
    }

    fun inc(x: &mut u64): u64 {
        *x = *x + 1;
        *x
    }

    fun inc_by(x: &mut u64, y: u64): u64 {
        *x = *x + y;
        *x
    }

    public fun test4(): u64 {
        let x = 1;
        let s = S { x, y: inc(&mut x), z: inc(&mut x) };
        let x;
        let y;
        let z;
        S {x, y, z} = s;
        x + y + z
    }

    public fun test5(): u64 {
        let x = 1;
        let s = S { x, y: {x = x + 1; x}, z: {x = x + 1; x} };
        let S {x, y, z} = s;
        x + y + z
    }

    public fun test6(): u64 {
        let x = 1;
        let S {x, y, z} = S { x, y: inc_by(&mut x, 7), z: inc_by(&mut x, 11) };
        x + y + z
    }

    public fun test7(): u64 {
        let x = 1;
        let S {x, y, z} = S { x, y: {x = x + 1; x}, z: {x = x + 1; x} };
        x + y + z
    }
}

//# run
module 0x043::test1 { public fun main() { assert!(0x42::m::test1() == 6, 1); } }

//# run
module 0x044::test2 { public fun main() { assert!(0x42::m::test2() == 20, 2); } }

//# run
module 0x045::test3 { public fun main() { assert!(0x42::m::test3() == 6, 3); } }

//# run
module 0x046::test4 { public fun main() { assert!(0x42::m::test4() == 6, 4); } }

//# run
module 0x047::test5 { public fun main() { assert!(0x42::m::test5() == 6, 5); } }

//# run
module 0x048::test6 { public fun main() { assert!(0x42::m::test6() == 28, 6); } }

//# run
module 0x049::test7 { public fun main() { assert!(0x42::m::test7() == 6, 7); } }



module a::m {

    fun simple(x: u32) {
        0 as u32;
        0u32 as u64;
        x as u64;
        foo() as u64;
        id(1 << 63) as u32;
        { x + 1 } as u64;
        (x + 1) as u64;
        (x: u32) as u64;
        copy x as u64;
        move x as u64;
    }

    fun foo(): u32 { 0 }
    fun id<T>(x: T): T { x }

    public struct S {
        f: u64,
    }

    // valid syntax
    fun scratch_work(cond: bool, x: u64, r: &u64, s: &S) {
        x as u32;
        if (cond) x as u32 else 0;
        if (cond) 0 else x as u32;
        *r as u32;
        s.f as u32;
        (x + 1u64 / 2 * 10) as u32;
        1u64 as u32 + 2;
        (x + 1 as u32);
        { x as u32 };
        (1u64 + x) as u64;
        let _: bool = x as u32 == x as u32;
        let _: bool = (1 + x) as u32 + 1 == x as u32 + 1;
        let _: bool = (1u64 + x) as u32 + 1 == x as u32 + 1 && x as u32 == 1;
        abort 1u32 as u64
    }

    fun ret(cond: bool): u64 {
        if (cond) return 1u32 as u64;
        0
    }

}

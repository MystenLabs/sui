module a::m {
    struct S {
        f: u64,
    }

    // valid syntax
    fun t(cond: bool, x: u64, r: &u64, s: &S): u64 {
        x as u32;
        if (cond) x as u32 else 0;
        if (cond) 0 else x as u32;
        *r as u32;
        s.f as u32;
        1u64 as u32 + 2;
        (x + 1 as u32);
        { x as u32 };
        abort 1u32 as u64;
        return 1u32 as u64
    }

    // valid syntax, but invalid types
    fun invalid_types(cond: bool, x: u64) {
        while (cond) x as u32;
        loop x as u32;
    }
}

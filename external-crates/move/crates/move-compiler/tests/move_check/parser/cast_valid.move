module a::m {
    // valid syntax
    fun t(cond: bool, x: u64) {
        x as u32;
        if (cond) x as u32 else 0;
        if (cond) 0 else x as u32;
        1u64 + 2u64 as u32;
        1u64 as u32 + 2u64;
    }

    // valid syntax, but invalid types
    fun invalid_types(cond: bool, x: u64) {
        while (cond) x as u32;
        loop x as u32;
    }
}

// cases are valid for 'as' but do not type check
module a::m {
    fun invalid_types(cond: bool, x: u64) {
        while (cond) x as u32;
        loop x as u32;
        cond && x as u8;
        cond || x as u8;
        x as u8 && cond;
        x as u8 || cond;
        (!x as u32);
        (!x) as u32;
    }
}

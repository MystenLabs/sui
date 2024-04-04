module a::m {
    // $e is given a new type on each usage... its weird
    macro fun weird($e: _): _ {
        // RHS usage is always u8
        // LHS usage is always the inferred return type
        $e << $e
    }

    fun t() {
        let _: u8 = weird!(1);

        let _: u8 = weird!(1 + 1);
        let _: u16 = weird!(1 + 1);
        let _: u32 = weird!(1 + 1);
        let _: u64 = weird!(1 + 1);
        let _: u128 = weird!(1 + 1);
        let _: u256 = weird!(1 + 1);
    }
}

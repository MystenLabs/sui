module a::m {
    // $e is given a new type on each usage... its weird
    macro fun weird($e: _): _ {
        // RHS usage is always u8
        // LHS usage is always the inferred return type
        $e << $e
    }

    fun tweird() {
        let _: u8 = weird!(1);

        let _: u8 = weird!(1 + 1);
        let _: u16 = weird!(1 + 1);
        let _: u32 = weird!(1 + 1);
        let _: u64 = weird!(1 + 1);
        let _: u128 = weird!(1 + 1);
        let _: u256 = weird!(1 + 1);
    }

    macro fun woah($e: _): (_, _) {
        ($e, $e)
    }

    fun twoah() {
        let (_, _): (u64, u8) = woah!(1);

        let (_, _): (u8, u256) = woah!(1 + 1);
        let (_, _): (u16, u128) = woah!(1 + 1);
        let (_, _): (u32, u64) = woah!(1 + 1);
        let (_, _): (u64, u32) = woah!(1 + 1);
        let (_, _): (u128, u16) = woah!(1 + 1);
        let (_, _): (u256, u8) = woah!(1 + 1);
    }
}

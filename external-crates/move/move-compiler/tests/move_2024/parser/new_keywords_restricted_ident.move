module a::m {
    public fun foo(): u64 {
        let mut `mut` = 0;
        let mut `enum` = 0;
        let mut `type` = 0;
        let mut `match` = 0;
        `mut` +
        `enum`+
        `type` +
        `match` +
        0;
        `mut` = 1;
        `enum` = 1;
        `type` = 1;
        `match` = 1;
        `mut` +
        `enum` +
        `type` +
        `match` +
        0
    }
}

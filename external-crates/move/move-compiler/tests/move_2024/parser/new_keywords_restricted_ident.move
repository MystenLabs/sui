module a::m {
    public fun foo(): u64 {
        let `mut` = 0;
        let `enum` = 0;
        let `type` = 0;
        let `match` = 0;
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

module a::m {
    public fun foo(): u64 {
        let r#mut = 0;
        let r#enum = 0;
        r#mut +
        r#enum;
        r#mut = 1;
        r#enum = 1;
        r#mut +
        r#enum
    }
}

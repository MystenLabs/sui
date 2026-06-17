module 0x2a::M {
    const MY_CONST: u64 = 60;

    public fun f(): u64 {
        match (0u64) {
            MY_CONST => 1u64,
            y @ _ if (*y > 0) => 2u64,
            _ => 3u64
        }
    }
}

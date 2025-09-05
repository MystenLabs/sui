module a::m;

fun test(): bool {
    let result = 'a: {
        match (0u8) {
            255 => return 'a true,
            0 => return 'a false,
            _ => return 'a false,
        };
        true
    };
    result
}

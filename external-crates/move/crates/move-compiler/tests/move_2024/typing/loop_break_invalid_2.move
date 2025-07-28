module a::m;

fun test(): u64 {
    let x = loop {
        break 0u8
    };
    x
}

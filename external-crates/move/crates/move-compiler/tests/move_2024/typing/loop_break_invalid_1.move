module a::m;

fun test(): u64 {
    let x: u64 = loop {
        break 0u8
    };
    x
}

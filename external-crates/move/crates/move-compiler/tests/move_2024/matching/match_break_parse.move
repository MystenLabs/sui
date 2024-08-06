module a::m;

public fun t(): u64 {
    loop {
        break match (0u64) { 0 => 0, _ => 0 }
    }
}

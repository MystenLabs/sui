module 0x42::m;

public fun report_from_value(code: u64) {
    if (code < 10) abort 0 else abort 1
}

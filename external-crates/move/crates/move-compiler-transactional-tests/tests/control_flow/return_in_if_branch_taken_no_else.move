//# publish
module 0x42::Test {
    public fun t(): u64 {
        if (true) return 100;
        0
    }
}

//# run
module 0x42::m {
use 0x42::Test;

fun main() {
    assert!(Test::t() == 100, 42);
}
}

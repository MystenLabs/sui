//# publish
module 0x42::Test {
    public fun t(): u64 {
        let x;
        if (true) {
            return 100
        } else {
            x = 0;
        };
        x
    }
}

//# run
script {
use 0x42::Test;

fun main() {
    assert!(Test::t() == 100, 42);
}
}

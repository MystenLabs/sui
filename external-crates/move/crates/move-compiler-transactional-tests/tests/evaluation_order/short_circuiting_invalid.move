//# publish
module 0x42::X {
    public fun error(): bool {
        abort 42
    }
}

// all should abort

//# run
module 0x42::m {
use 0x42::X;
fun main() {
    false || X::error();
}
}


//# run
module 0x42::m {
use 0x42::X;
fun main() {
    true && X::error();
}
}

//# run
module 0x42::m {
use 0x42::X;
fun main() {
    X::error() && false;
}
}

//# run
module 0x42::m {
use 0x42::X;
fun main() {
    X::error() || true;
}
}

//# run
module 0x42::m {
fun main() {
    false || { abort 0 };
}
}

//# publish
module 0x42::X {
    public fun error(): bool {
        abort 42
    }
}

// all should abort

//# run
module 2::m {
use 0x42::X;
fun main() {
    false || X::error();
}
}


//# run
module 3::m {
use 0x42::X;
fun main() {
    true && X::error();
}
}

//# run
module 4::m {
use 0x42::X;
fun main() {
    X::error() && false;
}
}

//# run
module 5::m {
use 0x42::X;
fun main() {
    X::error() || true;
}
}

//# run
module 6::m {
fun main() {
    false || { abort 0 };
}
}

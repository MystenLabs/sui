//# publish
module 0x42::X {
    public fun error(): bool {
        abort 42
    }
}

// all should abort

//# run
script {
use 0x42::X;
fun main() {
    false || X::error();
}
}


//# run
script {
use 0x42::X;
fun main() {
    true && X::error();
}
}

//# run
script {
use 0x42::X;
fun main() {
    X::error() && false;
}
}

//# run
script {
use 0x42::X;
fun main() {
    X::error() || true;
}
}

//# run
script {
fun main() {
    false || { abort 0 };
}
}

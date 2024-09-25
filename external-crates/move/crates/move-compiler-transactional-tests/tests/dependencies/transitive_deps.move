//# publish
module 0x42::X {
    struct T has drop {}
    public fun new(): T {
        T {}
    }
}

//# publish
module 0x43::Y {
    use 0x42::X;
    public fun foo(): X::T {
        X::new()
    }
}


//# run
module 0x44::m {
use 0x43::Y;

fun main() {
    Y::foo();
}
}

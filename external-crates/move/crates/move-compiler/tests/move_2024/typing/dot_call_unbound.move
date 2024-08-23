module 0x42::t {

public struct X has drop {}

fun call(x: &X) {
    x.f();
}

}

module 0x42::m {

use 0x42::t::X;

public struct Y has drop { x: X }

public fun call(x: &X, y: Y) {
    x.f();
    y.x.f();
}

}

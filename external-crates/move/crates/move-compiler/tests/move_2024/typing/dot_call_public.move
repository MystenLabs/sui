module 0x42::t {

public struct X has drop {}

public fun pub(_self: &X) {}
public(package) fun fr(_self: &X) {}

}

module 0x42::m {

use 0x42::t::X;

public struct Y has drop { x: X }

public fun call(x: &X, y: Y) {
    x.pub();
    x.fr();
    y.x.pub();
    y.x.fr();
}

}

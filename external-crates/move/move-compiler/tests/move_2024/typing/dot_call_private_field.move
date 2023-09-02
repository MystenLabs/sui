module 0x42::t {

struct X has drop {}
struct Y has drop { x: X }

public fun f(_self: &X) {}

}

module 0x43::m {

public fun call_f(y: &0x42::t::Y) {
    y.x.f()
}

}

module 0x42::t {

struct X has copy, drop {}
struct Y has copy, drop { x: X }

fun f(_self: X) {}

public fun foo (x: X, y1: Y, y2: &Y) {
    x.f();
    y1.x.f();
    y2.x.f();
}

}

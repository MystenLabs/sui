module 0x42::t {

struct X has drop {}
struct Y has drop { x: X }

fun f(_self: &X) {}

public fun foo(x: X, x2: &X, x3: &mut X) {
    x.f();
    x2.f();
    x3.f();
}

public fun bar(y: Y, y2: &Y, y3: &mut Y) {
    y.x.f();
    y2.x.f();
    y3.x.f();
}

}

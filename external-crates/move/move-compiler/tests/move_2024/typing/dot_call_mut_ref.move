module 0x42::t {

struct X has drop {}
struct Y has drop { x: X }

fun f(_self: &mut X) {}

public fun foo(x: X, x2: &mut X) {
    x.f();
    x2.f();
}

public fun bar(y: Y, y2: &mut Y) {
    y.x.f();
    y2.x.f();
}

}

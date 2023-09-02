module 0x42::t {

struct X has drop {}
struct Y has drop { x: X }

fun f(_self: &mut X) {}

public fun foo(x: &X) {
    x.f();
}

public fun bar(y: &Y) {
    y.x.f();
}

}

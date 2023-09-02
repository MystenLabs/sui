module 0x42::t {

struct X has drop {}
struct Y has drop { x: X }
struct Z has drop {}

fun g(_self: Z) {}

public fun foo(x: &X) {
    x.g();
}

public fun bar(y: &Y) {
    y.x.g();
}

}

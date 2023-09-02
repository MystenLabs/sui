module 0x42::t {

struct X has drop {}
struct Y has drop { x: X }

fun h() {}

public fun foo(x: &X) {
    x.h();
}

public fun bar(y: &Y) {
    y.x.h();
}

}

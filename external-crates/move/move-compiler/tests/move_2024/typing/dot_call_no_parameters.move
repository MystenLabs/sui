module 0x42::t {

public struct X has drop {}
public struct Y has drop { x: X }

fun h() {}

public fun foo(x: &X) {
    x.h();
}

public fun bar(y: &Y) {
    y.x.h();
}

}

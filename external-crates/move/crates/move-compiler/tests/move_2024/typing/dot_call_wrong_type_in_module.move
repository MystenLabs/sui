module 0x42::t {

public struct X has drop {}
public struct Y has drop { x: X }
public struct Z has drop {}

fun g(_self: Z) {}

public fun foo(x: &X) {
    x.g();
}

public fun bar(y: &Y) {
    y.x.g();
}

}

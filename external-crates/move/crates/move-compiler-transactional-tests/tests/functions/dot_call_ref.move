//# init --edition 2024.alpha

//# publish

module 0x42::t {

public struct X has copy, drop {}
public struct Y has copy, drop { x: X }

public fun f(_self: &X): bool { true }

public fun owned(x: X, y: Y) {
    assert!(x.f(), 0);
    assert!(y.x.f(), 0);
}

public fun ref(x: &X, y: &Y) {
    assert!(x.f(), 0);
    assert!(y.x.f(), 0);
}

public fun mut_ref(x: &mut X, y: &mut Y) {
    assert!(x.f(), 0);
    assert!(y.x.f(), 0);
}

public fun tmp(x: &X, y: &Y) {
    assert!((*x).f(), 0);
    assert!((*&y.x).f(), 0);
}

public fun test() {
    let mut x = X{};
    let mut y = Y { x };
    ref(&x, &y);
    mut_ref(&mut x, &mut y);
    owned(x, y)
}

}

//# run 0x42::t::test

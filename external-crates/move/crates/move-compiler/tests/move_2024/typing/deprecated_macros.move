module 0x42::m {
    #[deprecated]
    public struct X() has drop;

    public struct Y<phantom T>() has drop;

    #[deprecated]
    fun f() { }

    macro fun call_with_x($f: || -> X): X {
        f();
        $f()
    }

    macro fun call<$T>($f: || -> $T): $T {
        f();
        $f()
    }

    macro fun call_unders($f: || -> _): _ {
        f();
        $f()
    }

    fun call_call_with_x() {
        f();
        call_with_x!(|| X());
    }

    fun call_call() {
        f();
        call!(|| X());
        call!(|| Y<X>());
        call!(|| Y<Y<X>>());
    }

    fun call_call_unders() {
        f();
        call_unders!(|| X());
        call_unders!(|| Y<X>());
        call_unders!(|| Y<Y<X>>());
    }
}

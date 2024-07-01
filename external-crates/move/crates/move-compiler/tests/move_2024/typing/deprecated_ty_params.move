module 0x42::m {
    #[deprecated]
    public struct X() has drop;

    public struct Y<phantom T>() has drop;


    fun f() {
        let _ = Y<X>();
        let Y<X>() = Y<X>();
    }

    fun g<T>() { }

    fun call_g() {
        g<X>();
        g<Y<X>>();
        g<Y<Y<Y<Y<X>>>>>();
    }
}

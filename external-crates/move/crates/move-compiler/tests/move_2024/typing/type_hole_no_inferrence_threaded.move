module a::m {
    public struct Cup<T>(T) has copy, drop, store;
    public struct X() has copy, drop, store;
    fun x(_: &X) {}
    fun foo() {
        let mut c: Cup<_> = any();
        loop {
            c.0.x();
            c = Cup(X());
        }
    }
    fun any<T>(): T { abort 0 }
}

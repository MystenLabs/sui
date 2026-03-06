module 0x42::m {
    public struct A() has drop;

    fun f() {
        A(());
    }
}

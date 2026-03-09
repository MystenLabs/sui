module 0x0::m {
    fun id<T>(x: &T): &T { x }

    fun t(x: &u64) {
        id(x).*x;
    }
}

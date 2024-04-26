module a::m {
    fun id<T>(x: T): T {
        x
    }

    fun foo(): vector<u64> {
        let v: vector<_> = id<_>(vector<_>[]);
        v
    }
}

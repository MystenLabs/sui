module a::m {
    public struct Cup<T> {
        value: T
    }
    public fun uncup<T>(c: Cup<T>): T {
        let Cup { value } = c;
        value
    }

    fun t() {
        let c = Cup { value: 0 };
        c.uncup<u64> ();
    }
}

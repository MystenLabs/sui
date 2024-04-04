module a::m {
    fun foo() {
        let _: vector<_> = vector[];
        any<_>();
    }

    fun any<T>(): T { abort 0 }

}

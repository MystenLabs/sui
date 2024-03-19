module a::m {
    public entry fun foo<T>(_: vector<T>) {
        abort 0
    }
}

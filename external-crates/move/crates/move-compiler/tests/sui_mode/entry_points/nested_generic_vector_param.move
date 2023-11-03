// can have nested primitive vectors
module a::m {
    public entry fun foo<T>(_: vector<vector<T>>) {
        abort 0
    }
}

// cannot have nested object vectors
module a::m {
    public entry fun foo<T: key>(_: vector<vector<T>>) {
        abort 0
    }
}

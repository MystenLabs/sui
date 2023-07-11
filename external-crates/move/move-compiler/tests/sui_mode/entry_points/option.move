// valid, option of primitives is allowed

module a::m {
    use std::option;

    public entry fun yes<T>(
        _: option::Option<u64>,
        _: option::Option<option::Option<u64>>,
        _: option::Option<vector<u64>>,
        _: vector<option::Option<u64>>,
        _: option::Option<option::Option<T>>,
    ) {
        abort 0
    }

}

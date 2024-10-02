// invalid, type parameters with key are not valid when nested as no primitive has key

module a::m {
    use std::option;

    public entry fun t<T:key>(_: option::Option<T>) {
        abort 0
    }

    public entry fun t2<T:key>(_: vector<option::Option<T>>) {
        abort 0
    }

}

// valid, type parameters with key are valid as long as they are not nested

module a::m {
    public entry fun yes<T:key>(_: T, _: &T, _: &mut T) {
        abort 0
    }

}

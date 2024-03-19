// invalid, a mutable reference to vector of objects

module a::m {

    public entry fun no<T:key>(_: &mut vector<T>) {
        abort 0
    }

}

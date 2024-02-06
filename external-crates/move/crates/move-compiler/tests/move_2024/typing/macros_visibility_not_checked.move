module a::m {

    fun private() {}
    public(package) fun package() {}

    // these should be errors if we checked macros bodies before expanding

    public macro fun t0() {
        package();
        private();
    }

    public(package) fun t1() {
        private();
    }
}

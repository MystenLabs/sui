module a::m {

    fun private() {}
    public(package) fun package() {}

    // these will give errors after expanding

    public macro fun t0() {
        package();
        private();
    }

    public(package) macro fun t1() {
        private();
    }
}

// same package
module a::other {
    public fun call() {
        a::m::t0!();
        a::m::t1!();
    }
}

// different package
module b::other {
    public fun call() {
        a::m::t0!();
        a::m::t1!();
    }
}

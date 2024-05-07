module a::m {
    // this will give a type error when it is called
    macro fun bad() {
        1 + b"2"
    }

    fun t() {
        bad!();
        bad!(); // only one error expected since all of the source locations are the same
    }

}

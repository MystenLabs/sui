module a::m {
    // this should give an error if we checked macros bodies before expanding
    macro fun bad() {
        1 + b"2"
    }
}

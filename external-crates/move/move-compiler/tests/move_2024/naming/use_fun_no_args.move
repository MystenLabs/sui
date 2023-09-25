module a::m {
    struct Y {}

    public fun no() { abort 0 }

    use fun no as Y.no;
}

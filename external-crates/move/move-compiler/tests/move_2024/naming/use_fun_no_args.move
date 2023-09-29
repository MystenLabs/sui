module a::m {
    public struct Y {}

    public fun no() { abort 0 }

    use fun no as Y.no;
}

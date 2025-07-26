// tests cases where a lint is reported for a redundant conditional expression
module a::m {
    public fun t0(condition: bool) {
        if (!condition) true else false;
        if (condition) { { false } } else { (true: bool) };
    }

    public fun t1() {
        if (true) true else true;
        if (foo()) 0 else 0;
        if (!foo()) b"" else x"";
    }

    fun foo(): bool { true }
}

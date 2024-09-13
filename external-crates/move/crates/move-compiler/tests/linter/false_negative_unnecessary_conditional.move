module a::m {
    // These very simply could be rewritten but we are overly conservative when it comes to blocks
    public fun t0(condition: bool) {
        if (condition) { (); true } else false;
        if (condition) b"" else { (); (); vector[] };
    }

    // we don't do this check after constant folding
    public fun t1(condition: bool) {
        if (condition) 1 + 1 else 2;
    }
}

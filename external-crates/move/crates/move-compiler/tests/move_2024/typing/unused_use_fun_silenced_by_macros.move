// explicit unused, silenced by a public macro
module a::m {
    fun foo(_: u64) {}

    use fun foo as u64.f;

    fun t() {
        // this one will still be unused since it is not in the module scope
        use fun foo as u64.f;
    }

    // this silences the unused use fun in the module, since we lose track its non-local usage
    public(package) macro fun some_macro() {}
}

// implicit unused method alias, silenced by a public macro
module a::x {
    public struct X() has drop;
    public fun drop(_: X) {}
}

module b::other {
    use a::x::drop as f;

    fun t() {
        // this one will still be unused since it is not in the module scope
        use a::x::drop as f;
    }

    // this silences the unused use fun in the module, since we lose track its non-local usage
    public macro fun some_macro() {}
}

// explicit and implicit method alias, silenced by a private macro
module a::another {
    use a::x::drop as fdrop;

    fun foo(_: u64) {}

    use fun foo as u64.foo;

    macro fun f() {}
}

// tests modules cannot emit authenticated events for types not defined in the current module
module a::m {
    use sui::event;

    struct X has copy, drop {}

    public fun t(s: a::other::Event) {
        event::emit_authenticated(s)
    }

    public fun gen<T: copy + drop>(x: T) {
        event::emit_authenticated(move x)
    }

    public fun prim(x: u64) {
        event::emit_authenticated(x)
    }

    public fun vec(x: vector<X>) {
        event::emit_authenticated(move x)
    }
}

module a::other {
    struct Event has copy, drop {}
}

module sui::event {
    public fun emit<T: copy + drop>(_: T) { abort 0 }

    public fun emit_authenticated<T: copy + drop>(_: T) { abort 0 }
}

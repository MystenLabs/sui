// tests modules cannot emit events for types not defined in the current module
module a::m {
    use sui::event;

    struct X has copy, drop {}

    public fun t(s: a::other::Event) {
        event::emit(s)
    }

    public fun gen<T: copy + drop>(x: T) {
        event::emit(move x)
    }

    public fun prim(x: u64) {
        event::emit(x)
    }

    public fun vec(x: vector<X>) {
        event::emit(move x)
    }
}

module a::other {
    struct Event has copy, drop {}
}

module sui::event {
    public fun emit<T: copy + drop>(_: T) { abort 0 }
}

module auth_event::events {
    use sui::event;

    public struct E has copy, drop { value: u64 }

    public entry fun emit(value: u64) {
        event::emit_authenticated(E { value });
    }
}



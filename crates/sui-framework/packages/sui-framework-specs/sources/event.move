module specs::event_spec;

use sui::event::emit;

public struct SpecEvent has copy, drop {
    value: u64
}

#[spec(target = sui::event::emit)]
public fun emit_spec<T: copy + drop>(event: T) {
    emit(SpecEvent { value: 0 })
}

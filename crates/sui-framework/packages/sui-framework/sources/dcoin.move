module sui::dcoin;

use std::type_name::{Self, TypeName};

use sui::dynamic_field;
use sui::balance::Balance;

/// A balance whose type is carried in a field rather than a phantom type parameter
public struct DCoin has key, store {
    id: UID,
    // TODO: could also issue unique int at registration time and use that instead
    typ: TypeName,
    value: u64
    // TODO: decimals also?
}

public struct Registry has key {
    id: UID,
    // carries dynamic fields mapping TypeName -> Balance<TypeName>
    // note: might want a single global Registry, but separate exchange pool for each coin type to avoid contention

}

fun init(ctx: &mut TxContext) {
    transfer::share_object(Registry { id: object::new(ctx)} )
}

public fun new<T>(registry: &mut Registry, b: Balance<T>, ctx: &mut TxContext): DCoin {
    let typ = type_name::get<T>();
    let value = b.value();
    if (!dynamic_field::exists_(&registry.id, typ)) {
        dynamic_field::add(&mut registry.id, typ, b)
    } else {
        dynamic_field::borrow_mut<TypeName, Balance<T>>(&mut registry.id, typ).join(b);
    };
    DCoin { id: object::new(ctx), typ, value }
}

public fun delete<T>(registry: &mut Registry, c: DCoin): Balance<T> {
    let DCoin { id, typ, value } = c;
    id.delete();
    dynamic_field::borrow_mut<TypeName, Balance<T>>(&mut registry.id, typ).split(value)
}

// ... split, join, other coin ops on DCoin, similar DBalance type


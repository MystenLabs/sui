---
title: Chapter 5 - Dynamic Fields
---

Previous chapters describe various ways to use object fields to store primitive data and other objects (wrapping), but there are a few limitations to this approach:

1. Object's have a finite set of fields keyed by identifiers that are fixed when its module is published (i.e. limited to the fields in the `struct` declaration).
2. An object can become very large if it wraps several other objects. Larger objects can lead to higher gas fees in transactions. In addition, there is an upper bound on object size.
3. Later chapters include use cases where you need to store a collection of objects of heterogeneous types. Since the Sui Move `vector` type must be instantiated with one single type `T`, it is not suitable for this.

Fortunately, Sui provides *dynamic fields* with arbitrary names (not just identifiers), added and removed on-the-fly (not fixed at publish), which only affect gas when they are accessed, and can store heterogeneous values. This chapter introduces the libraries for interacting with this kind of field.

### Fields vs Object Fields

There are two flavors of dynamic field -- "fields" and "object fields" -- which differ based on how their values are stored:

- **Fields** can store any value that has `store`, however an object stored in this kind of field will be considered wrapped and will not be accessible via its ID by external tools (explorers, wallets, etc) accessing storage.
- **Object field** values *must* be objects (have the `key` ability, and `id: UID` as the first field), but will still be accessible at their ID to external tools.

The modules for interacting with these fields can be found at [`dynamic_field`](https://github.com/MystenLabs/sui/tree/main/crates/sui-framework/packages/sui-framework/sources/dynamic_field.move) and [`dynamic_object_field`](https://github.com/MystenLabs/sui/tree/main/crates/sui-framework/packages/sui-framework/sources/dynamic_object_field.move) respectively.

### Field Names

Unlike an object's regular fields whose names must be Move identifiers, dynamic field names can be any value that has `copy`, `drop` and `store`. This includes all Move primitives (integers, booleans, byte strings), and structs whose contents all have `copy`, `drop` and `store`.

### Adding Dynamic Fields

Dynamic fields are added with the following APIs:

```rust
module sui::dynamic_field {

public fun add<Name: copy + drop + store, Value: store>(
  object: &mut UID,
  name: Name,
  value: Value,
);

}
```

```rust
module sui::dynamic_object_field {

public fun add<Name: copy + drop + store, Value: key + store>(
  object: &mut UID,
  name: Name,
  value: Value,
);

}
```

These functions add a field with name `name` and value `value` to `object`. To see it in action, consider these code snippets:

First, define two object types for the parent and the child:

```rust
struct Parent has key {
    id: UID,
}

struct Child has key, store {
    id: UID,
    count: u64,
}
```

Next, define an API to add a `Child` object as a dynamic field of a `Parent` object:

```rust
use sui::dynamic_object_field as ofield;

public entry fun add_child(parent: &mut Parent, child: Child) {
    ofield::add(&mut parent.id, b"child", child);
}
```

This function takes the `Child` object by value and makes it a dynamic field of `parent` with name `b"child"` (a byte string of type `vector<u8>`). This call results in the following ownership relationship:

1. Sender address (still) owns the `Parent` object.
2. The `Parent` object owns the `Child` object, and can refer to it by the name `b"child"`.

It is an error to overwrite a field (attempt to add a field with the same Name type and value as one that is already defined), and a transaction that does this will fail.  Fields can be modified in-place by borrowing them mutably and can be overwritten safely (such as to change its value type) by removing the old value first.

### Accessing Dynamic Fields

Dynamic fields can be accessed by reference using the following APIs:

```rust
module sui::dynamic_field {

public fun borrow<Name: copy + drop + store, Value: store>(
    object: &UID,
    name: Name,
): &Value;

public fun borrow_mut<Name: copy + drop + store, Value: store>(
    object: &mut UID,
    name: Name,
): &mut Value;

}
```

Where `object` is the UID of the object the field is defined on and `name` is the field's name.

**Note:** `sui::dynamic_object_field` has equivalent functions for object fields, but with the added constraint `Value: key + store`.

To use these APIs with the `Parent` and `Child` types defined earlier:

```rust
use sui::dynamic_object_field as ofield;

public entry fun mutate_child(child: &mut Child) {
    child.count = child.count + 1;
}

public entry fun mutate_child_via_parent(parent: &mut Parent) {
    mutate_child(ofield::borrow_mut<vector<u8>, Child>(
        &mut parent.id,
        b"child",
    ));
}
```

The first function accepts a mutable reference to the `Child` object directly, and can be called with `Child` objects that haven't been added as fields to `Parent` objects.

The second function accepts a mutable reference to the `Parent` object and accesses its dynamic field using `borrow_mut`, to pass to `mutate_child`. This can only be called on `Parent` objects that have a `b"child"` field defined. A `Child` object that has been added to a `Parent` *must* be accessed via its dynamic field, so it can only by mutated using `mutate_child_via_parent`, not `mutate_child`, even if its ID is known.

**Important:** A transaction that attempts to borrow a field that does not exist will fail.

The `Value` type passed to `borrow` and `borrow_mut` must match the type of the stored field, or the transaction will abort.

Dynamic object field values *must* be accessed through these APIs.  A transaction that attempts to use those objects as inputs (by value or by reference), will be rejected for having invalid inputs.

### Removing a Dynamic Field

Similar to unwrapping, an object held in a regular field, a dynamic field can be removed, exposing its value:

```rust
module sui::dynamic_field {

public fun remove<Name: copy + drop + store, Value: store>(
    object: &mut UID,
    name: Name,
): Value;

}
```

This function takes a mutable reference to the ID of the `object` the field is defined on, and the field's `name`.  If a field with a `value: Value` is defined on `object` at `name`, it will be removed and `value` returned, otherwise it will abort.  Future attempts to access this field on `object` will fail.

> :bulb: `sui::dynamic_object_field` has an equivalent function for object fields.

The value that is returned can be interacted with just like any other value (because it is any other value). For example, removed dynamic object field values can then be `delete`-d or `transfer`-ed to an address (back to the sender):

```rust
use sui::dynamic_object_field as ofield;
use sui::{object, transfer, tx_context};
use sui::tx_context::TxContext;

public entry fun delete_child(parent: &mut Parent) {
    let Child { id, count: _ } = ofield::remove<vector<u8>, Child>(
        &mut parent.id,
        b"child",
    );

    object::delete(id);
}

public entry fun reclaim_child(parent: &mut Parent, ctx: &mut TxContext) {
    let child = ofield::remove<vector<u8>, Child>(
        &mut parent.id,
        b"child",
    );

    transfer::transfer(child, tx_context::sender(ctx));
}
```

Similar to borrowing a field, a transaction that attempts to remove a non-existent field, or a field with a different `Value` type, fails.

### Deleting an Object with Dynamic Fields

It is possible to delete an object that has dynamic fields still defined on it. Because field values can be accessed only via the dynamic field's associated object and field name, deleting an object that has dynamic fields still defined on it renders them all inaccessible to future transactions. This is true regardless of whether the field's value has the `drop` ability.

Deleting an object that has dynamic fields still defined on it is permitted, but it will render all its fields inaccessible.

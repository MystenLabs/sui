# Object Display

A creator or a builder who owns a `Publisher` object can use the `sui::display` module to define display properties for their objects. To get a Publisher object check out [the Publisher page](./publisher.md).

`Display<T>` is an object that specifies a set of named templates for the type `T` (for example, for a type `0x2::capy::Capy` the display would be `Display<0x2::capy::Capy>`). All objects of the type `T` will be processed in the Sui Full Node RPC through the matching `Display` definition and will have processed result attached when an object is queried.

## Description

Sui Object Display is a template engine which allows for on-chain display configuration for type to be handled off-chain by the ecosystem. It has the ability to use an object's data for substitution into a template string.

There's no limitation to what fields can be set, all object properties can be accessed via the `{property}` syntax and inserted as a part of the template string (see examples for the illustration).

## Example

For the following Hero module, the Display would vary based on the "name", "id" and "img_url" properties of the type "Hero". The template defined in the init function can be represented as:

```json
{
    "name": "{name}",
    "link": "https://sui-heroes.io/hero/{id}",
    "img_url": "ipfs://{img_url}",
    "description": "A true Hero of the Sui ecosystem!",
    "project_url": "https://sui-heroes.io",
    "creator": "Unknown Sui Fan"
}
```

```move
{{#include ../../examples/sources/basics/display.move:4:}}
```

## Methods description

Display is created via the `display::new<T>` call, which can be performed either in a custom function (or a module initializer) or as a part of a programmable transaction.

```move
module sui::display {
    /// Get a new Display object for the `T`.
    /// Publisher must be the publisher of the T, `from_package`
    /// check is performed.
    public fun new<T>(pub: &Publisher): Display<T> { /* ... */ }
}
```

Once acquired, the Display can be modified:
```move
module sui::display {
    /// Sets multiple fields at once
    public fun add_multiple(
        self: &mut Display,
        keys: vector<String>,
        values: vector<String
    ) { /* ... */ }

    /// Edit a single field
    public fun edit(self: &mut Display, key: String, value: String) { /* ... */ }

    /// Remove a key from Display
    public fun remove(self: &mut Display, key: String ) { /* ... */ }
}
```

To apply changes and set the Display for the T, one last call is required: `update_version` publishes version by emitting an event which Full Node listens to and uses to get a template for the type.
```move
module sui::display {
    /// Update the version of Display and emit an event
    public fun update_version(self: &mut Display) { /* ... */ }
}
```

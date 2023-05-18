---
title: Sui Object Display Standard
---

The Sui Object Display standard is a template engine that allows for on-chain management of off-chain representation (display) for a type. With it, you can substitute data for an object into a template string. The standard doesn’t limit the fields you can set. You can use the  `{property}` syntax to access all object properties, and then insert them as a part of the template string.

Use a `Publisher` object that you own to set `sui::display` for a type. For more information about `Publisher` objects, see [Publisher](https://examples.sui.io/basics/publisher.html) topic in *Sui Move by Example*.

In Sui Move, `Display<T>` represents an object that specifies a set of named templates for the type `T`. For example, for a type `0x2::capy::Capy` the display syntax is:  `Display<0x2::capy::Capy>`.

Sui Full nodes process all objects of the type `T` by matching the `Display` definition, and return the processed result when you query an object with the `{ showDisplay: true }` setting in the query.

## Display properties

The basic set of properties suggested includes:
**name** - A name for the object. The name is displayed when users view the object.
**description** - A description for the object. The description is displayed when users view the object.
**link** - A link to the object to use in an application.
**image_url** - A URL or a blob with the image for the object.
**thumbnail_url** - A URL to a **smaller** image to use in wallets, explorers, and other products as a preview.
**project_url** - A link to a website associated with the object or creator.
**creator** - A string that indicates the object creator.


### An example Sui Hero module
The following code sample demonstrates how the `Display` for an example `Hero` module varies based on the `name`, `id`, and `image_url` properties of the type `Hero`.
The following represents the template the `init` function defines:

```json
{
    "name": "{name}",
    "link": "https://sui-heroes.io/hero/{id}",
    "image_url": "ipfs://{img_url}",
    "description": "A true Hero of the Sui ecosystem!",
    "project_url": "https://sui-heroes.io",
    "creator": "Unknown Sui Fan"
}
```

```rust
/// Example of an unlimited "Sui Hero" collection - anyone can
/// mint their Hero. Shows how to initialize the `Publisher` and how
/// to use it to get the `Display<Hero>` object - a way to describe a
/// type for the ecosystem.
module examples::my_hero {
    use sui::tx_context::{sender, TxContext};
    use std::string::{utf8, String};
    use sui::transfer::transfer;
    use sui::object::{Self, UID};

    // The creator bundle: these two packages often go together.
    use sui::package;
    use sui::display;

    /// The Hero - an outstanding collection of digital art.
    struct Hero has key, store {
        id: UID,
        name: String,
        img_url: String,
    }

    /// One-Time-Witness for the module.
    struct MY_HERO has drop {}

    /// In the module initializer one claims the `Publisher` object
    /// to then create a `Display`. The `Display` is initialized with
    /// a set of fields (but can be modified later) and published via
    /// the `update_version` call.
    ///
    /// Keys and values are set in the initializer but could also be
    /// set after publishing if a `Publisher` object was created.
    fun init(otw: MY_HERO, ctx: &mut TxContext) {
        let keys = vector[
            utf8(b"name"),
            utf8(b"link"),
            utf8(b"image_url"),
            utf8(b"description"),
            utf8(b"project_url"),
            utf8(b"creator"),
        ];

        let values = vector[
            // For `name` one can use the `Hero.name` property
            utf8(b"{name}"),
            // For `link` one can build a URL using an `id` property
            utf8(b"https://sui-heroes.io/hero/{id}"),
            // For `image_url` use an IPFS template + `img_url` property.
            utf8(b"ipfs://{img_url}"),
            // Description is static for all `Hero` objects.
            utf8(b"A true Hero of the Sui ecosystem!"),
            // Project URL is usually static
            utf8(b"https://sui-heroes.io"),
            // Creator field can be any
            utf8(b"Unknown Sui Fan")
        ];

        // Claim the `Publisher` for the package!
        let publisher = package::claim(otw, ctx);

        // Get a new `Display` object for the `Hero` type.
        let display = display::new_with_fields<Hero>(
            &publisher, keys, values, ctx
        );

        // Commit first version of `Display` to apply changes.
        display::update_version(&mut display);

        transfer(publisher, sender(ctx));
        transfer(display, sender(ctx));
    }

    /// Anyone can mint their `Hero`!
    public fun mint(name: String, img_url: String, ctx: &mut TxContext): Hero {
        let id = object::new(ctx);
        Hero { id, name, img_url }
    }
}
```

## Work with Object Display

The `display::new<T>` call creates a `Display`, either in a custom function or module initializer, or as part of a programmable transaction.
The following code sample demonstrates how to create a `Display`:

```rust
module sui::display {
    /// Get a new Display object for the `T`.
    /// Publisher must be the publisher of the T, `from_package`
    /// check is performed.
    public fun new<T>(pub: &Publisher): Display<T> { /* ... */ }
}
```

After you create the `Display`, you can modify it. The following code sample demonstrates how to modify a `Display`:

```rust
module sui::display {
    /// Sets multiple fields at once
    public fun add_multiple(
        self: &mut Display,
        keys: vector<String>,
        values: vector<String>
    ) { /* ... */ }

    /// Edit a single field
    public fun edit(self: &mut Display, key: String, value: String) { /* ... */ }

    /// Remove a key from Display
    public fun remove(self: &mut Display, key: String ) { /* ... */ }
}
```

Next, the `update_version` call applies the changes and sets the `Display` for the `T` by emitting an event. Full nodes receive the event and use the data in the event to retrieve a template for the type.

The following code sample demonstrates how to use the `update_version` call:

```rust
module sui::display {
    /// Update the version of Display and emit an event
    public fun update_version(self: &mut Display) { /* ... */ }
}
```

## Sui utility objects

In Sui, utility objects enable authorization for capabilities. Almost all modules have features that can be accessed only with the required capability. Generic modules allow one capability per application, such as a marketplace. Some capabilities mark ownership of a shared object on-chain, or access the shared data from another account.
With capabilities, it is important to provide a meaningful description of objects to facilitate user interface implementation. This helps avoid accidentally transferring the wrong object when objects are similar. It also provides a user-friendly description of items that users see.

The following example demonstrates how to create a capy capability:

```rust
module capy::utility {
   /// A capability which grants Capy Manager permission to add
   /// new genes and manage the Capy Market
   struct CapyManagerCap has key, store {
    id: UID }
}
```

## Typical objects with data duplication

A common case with in-game items is to have a large number of similar objects grouped by some criteria. It is important to optimize their size and the cost to mint and update them. Typically, a game uses a single source image or URL per group or item criteria. Storing the source image inside of every object is not optimal.
In some cases, users mint in-game items when a game allows them or when they purchase an in-game item. To enable this, some IPFS/Arweave metadata must be created and stored in advance. This requires additional logic that is usually not related to the in-game properties of the item.

The following example demonstrates how to create a Capy:

```rust
module capy::capy_items {
   /// A wearable Capy item. For some items there can be an
   /// unlimited supply. And items with the same name are identical.
   struct CapyItem has key, store {
        id: UID,
        name: String
   }
}
```

## Unique objects with dynamic representation

Sui Capys use dynamic image generation. When a Capy is born, its attributes determine the Capy’s appearance, such as color or pattern. When a user puts an item on a Capy, the Capy’s appearance changes. When users put multiple items on a Capy, there’s a chance of a bonus for a combination of items.

To implement this, the Capys game API service refreshes the image in response to a user-initiated change. The URL for a Capy is a template with the `capy.id`. But storing the full URL - as well as other fields in the Capy object due to their diverse population - also leads to users paying for excess storage and increased gas fees.

The following example demonstrates how to implement dynamic image generation:

```rust
module capy::capy {
   /// A Capy - very diverse object with different combination
   /// of genes. Created dynamically + for images a dynamic SVG
   /// generation is used.
   struct Capy has key, store {
    id: UID,
    genes: vector<u8>
   }
}
```

## Objects with unique static content

This is the simplest scenario - an object represents everything itself. It is very easy to apply a metadata standard to an object of this kind, especially if the object stays immutable forever. However, if the metadata standard evolves and some ecosystem projects add new features for some properties, this object always stays in its original form and might require backward-compatible changes.

```rust
module sui::devnet_nft {
   /// A Collectible with a static data. URL, name, description are
   /// set only once on a mint event
   struct DevNetNFT has key, store {
       id: UID,
       name: String,
       description: String,
       url: Url,
   }
}
```

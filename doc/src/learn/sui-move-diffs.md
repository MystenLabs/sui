---
title: How Sui Move differs from Core Move
---

This document describes the Sui Move programming model and highlights the differences between the core (previously Diem) Move language and the Move used in Sui.

To learn more about the motivations behind creating Sui Move, see [Why We Created Sui Move](https://medium.com/mysten-labs/why-we-created-sui-move-6a234656c36b).

In general, Move code written for other systems will work in Sui with these exceptions:

 * [Global Storage operators](https://move-language.github.io/move/global-storage-operators.html)
 * [Key Abilities](https://github.com/move-language/move/blob/main/language/documentation/book/src/abilities.md#key)

Here is a summary of key differences:

 1. Sui uses its own [object-centric global storage](#object-centric-global-storage)
 1. Addresses [represent Object IDs](#addresses-represent-object-ids)
 1. Sui objects have [globally unique IDs](#object-with-key-ability-globally-unique-ids)
 1. Sui has [module initializers (init)](#module-initializers)
 1. Sui [entry points take object references as input](#entry-points-take-object-references-as-input)

Find a detailed description of each change below.

## Object-centric global storage

In core Move, global storage is part of the programming model and can be accessed through special operations, such as _move_to_, _move_from and_ many more[ global storage operators](https://move-language.github.io/move/global-storage-operators.html). Both resources and modules are stored in the core Move global storage. When you publish a module, it’s stored into a newly generated module address inside Move. When a new object (a.k.a. resource) is created, it's usually stored into some address, as well.

But on-chain storage is expensive and limited (not optimized for storage and indexing). Current blockchains cannot scale to handle storage-heavy applications such as marketplaces and social apps.

So there is no global storage in Sui Move. None of the global storage-related operations are allowed in Sui Move. (There is a bytecode verifier for this to detect violations.) Instead, storage happens exclusively within Sui. When you publish a module, the newly published module is stored in Sui storage, instead of Move storage. Similarly, newly created objects are stored in Sui storage. _This also means that when you need to read an object in Move, you cannot rely on global storage operations but instead Sui must explicitly pass all objects that need to be accessed into Move._

## Addresses represent Object IDs

In Move, there is a special _address_ type. This type is used to represent addresses in core Move. Core Move needs to know the address of an account when dealing with the global storage. The _address_ type is 16 bytes, which is sufficient for the core Move security model.

In Sui, since it doesn’t support global storage in Move, you don’t need the _address_ type to represent user accounts. Instead, use the _address_ type to represent the Object ID. Refer to the [object.move](https://github.com/MystenLabs/sui/tree/main/crates/sui-framework/packages/sui-framework/sources/object.move) file in Sui framework for an understanding of address use.

## Object with key ability, globally unique IDs

You need a way to distinguish between objects that are internal to Sui Move and objects that can be stored in Sui storage. This is important because you need to be able to serialize/deserialize objects in the Move-Sui boundary, and this process makes assumptions on the shape of the objects.

You can take advantage of the _key_ ability in Move to annotate a Sui object. In core Move, the [key ability](https://github.com/move-language/move/blob/main/language/documentation/book/src/abilities.md#key) is used to tell that the type can be used as a key for global storage. Since you don’t touch global storage in Sui Move, you are able to repurpose this ability. Sui requires that any struct with key ability must start with an _id_ field with the _ID_ type. The ID type contains both the ObjectID and the sequence number (a.k.a. version). Sui has bytecode verifiers in place to make sure that the ID field is immutable and cannot be transferred to other objects (as each object must have a unique ID).

## Module initializers

As described in [Object-centric global storage](#object-centric-global-storage), Sui Move modules are published into Sui storage. A special initializer function optionally defined in a module is executed (once) at the time of module publication by the Sui runtime for the purpose of pre-initializing module-specific data (e.g., creating singleton objects). The initializer function must have the following properties in order to be executed at publication:

 * Name `init`
 * Single parameter of `&mut TxContext` type
 * No return values
 * Private

## Entry points take object references as input

Sui offers entry functions that can be called directly from Sui, in addition to functions callable from other functions. See [Entry functions](../build/move/index.md#entry-functions).

## Conclusion

In summary, Sui takes advantage of Move’s security and flexibility and enhances it with the features described above to vastly improve throughput, reduce delays in finality, and make Sui Move programming easier. Now see [how Sui works](how-sui-works.md). For full details, see the [Sui Smart Contracts Platform](https://github.com/MystenLabs/sui/blob/main/doc/paper/sui.pdf) white paper.

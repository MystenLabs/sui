---
title: Chapter 5 - Child Objects
---

In the previous chapter, we walked through various ways of wrapping an object in another object. There are a few limitations in object wrapping:
1. A wrapped object can be accessed only via its wrapper. It cannot be used directly in a transaction or queried by its ID (e.g., in the explorer).
2. An object can become very large if it wraps several other objects. Larger objects can lead to higher gas fees in transactions. In addition, there is an upper bound on object size.
3. As we will see in future chapters, there will be use cases where we need to store a collection of objects of heterogeneous types. Since the Move `vector` type must be templated on one single type `T`, it is not suitable for this.

Fortunately, Sui provides another way to represent object relationships: *an object can own other objects*. In the first chapter, we introduced libraries for transferring objects to an address. In this chapter, we will introduce libraries that allow you transfer objects to other objects.

### Create child objects

There are two ways of creating child objects which we describe in the following sections.

#### transfer_to_object
Assume we own two objects in our address. To make one object own the other object, we can use the following API in the [`transfer`](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/sources/transfer.move) library:
```rust
public fun transfer_to_object<T: key, R: key>(
    obj: T,
    owner: &mut R,
)
```
The first argument `obj` will become a child object of the second argument `owner`. `obj` must be passed by value, i.e. it will be fully consumed and cannot be accessed again within the same transaction (similar to `transfer` function). After calling this function, the on-chain owner metadata of `obj` will change to the ID of the `owner` object.

This begs the question, what happens if we attempt to delete the parent object? In the metadata for an object, Sui maintains a count of how many child objects a parent has. The parent object cannot be deleted, wrapped, or frozen while it still has children. Why? Without this limitation, we may end up in a situation where we deleted the parent object, but there are still some child objects; and these child objects will be locked forever, as we will explain in latter sections.

Let's look at some code. The full source code can be found in [object_owner.move](https://github.com/MystenLabs/sui/blob/main/crates/sui-core/src/unit_tests/data/object_owner/sources/object_owner.move).

First we define two object types for the parent and the child:
```rust
struct Parent has key {
    id: UID,
    child: Option<ID>,
}

struct Child has key {
    id: UID,
}
```
`Parent` type contains a `child` field that is an optional child reference to an object of `Child` type.
First we define an API to create an object of `Child` type:
```rust
public entry fun create_child(ctx: &mut TxContext) {
    transfer::transfer(
        Child { id: object::new(ctx) },
        tx_context::sender(ctx),
    );
}
```
The above function creates a new object of `Child` type and transfers it to the sender address of the transaction, i.e. after this call, the sender address owns the object.
Similarly, we can define an API to create an object of `Parent` type:
```rust
public entry fun create_parent(ctx: &mut TxContext) {
    let parent = Parent {
        id: object::new(ctx),
        child: option::none(),
    };
    transfer::transfer(parent, tx_context::sender(ctx));
}
```
Since the `child` field is `Option` type, we can start with `option::none()`.
Now we can define an API that makes an object of `Child` a child of an object of `Parent`:
```rust
public entry fun add_child(parent: &mut Parent, child: Child) {
    option::fill(&mut parent.child, object::id(&child));
    transfer::transfer_to_object(child, parent);
}
```
This function takes the `Child` object by value, fills the `child` field of `parent` with the `ID` of the `Child` object, and calls `transfer_to_object` to transfer the `Child` object to the `parent`.
At the end of the `add_child` call, we have the following ownership relationship:
1. Sender address (still) owns a `Parent` object.
2. The `Parent` object owns a `Child` object.

#### transfer_to_object_id
In the above example, `Parent` has an optional child field. What if the field is not optional? We must construct `Parent` with a `ID`. However, in order to have a valid `ID`, we have to transfer the child object to the parent object first. This creates a somewhat paradoxical situation. We cannot create the parent unless we have a valid `ID`, and we cannot have a valid `ID` unless we already have the parent object. To solve this problem, we can use a different API that allows you to transfer an object to an object ID, instead of to the object itself:
```rust
public fun transfer_to_object_id<T: key>(
    obj: T,
    owner_id: &mut UID,
);
```
To use this API, we don't need to create a parent object yet; we need only the `UID` of the parent object, which can be created in advance through `object::new(ctx)`. The function requires a mutable reference to the parent `UID` for two reasons. (1) it prevents children from being added to immutable objects (more on that later). (2) it gives the module that defines the parent object more control. Namely, it can expose a function to get an immutable reference to the `&UID` without worrying about external caller adding child objects.

Let's see how this is used in action. First we define another object type that has a non-optional child field:
```rust
struct AnotherParent has key {
    id: UID,
    child: ID,
}
```
And let's see how we define the API to create `AnotherParent` instance:
```rust
public entry fun create_another_parent(child: Child, ctx: &mut TxContext) {
    let id = object::new(ctx);
    let child_id = object::id(&child);
    transfer::transfer_to_object_id(child, &mut id);
    let parent = AnotherParent {
        id,
        child: child_id,
    };
    transfer::transfer(parent, tx_context::sender(ctx));
}
```
In the above function, we need to first create the ID of the new parent object. With the ID, we can then transfer the child object to it by calling `transfer_to_object_id`, thereby obtaining a reference `child_ref`. With both `id` and `child_ref`, we can create an object of `AnotherParent`, which we would eventually transfer to the sender's address.

> :bulb: If we wanted to ensure that the `ID` in the `child` field of `Parent` or `AnotherParent` actually referred to an object of type `Child`, we could use `TypedID` which is defined in the [typed_id module](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/sources/typed_id.move)

### Use Child Objects
We have explained in the first chapter that, in order to use an owned object, the object owner must be the transaction sender. What about objects owned by objects? We require that the object's owner object must also be passed as an argument in the Move call. For example, if object A owns object B, and object B owns object C, to be able to use C when calling a Move entry function, one must also pass B as an argument; and since B is an argument, A must also be an argument. This essentially means that to use an object, its entire ownership ancestor chain must be included, and the  owner address of the root ancestor must match the sender of the transaction.

Let's look at how we could use the child object created earlier. Let's define two entry functions:
```rust
public entry fun mutate_child(_child: &mut Child) {}
public entry fun mutate_child_with_parent(_child: &mut Child, _parent: &mut Parent) {}
```
The first function requires only one object argument, which is a `Child` object. The second function requires two arguments, a `Child` object and a `Parent` object. Both functions are made empty since what we care about here is not the mutation logic, but whether you are able to make a call to them at all.
Both functions will compile successfully, because object ownership relationships are dynamic properties and the compiler cannot forsee them.

Let's try to interact with these two entry functions on-chain and see what happens. First we publish the sample code:
```
$ sui client publish --path $ROOT/sui-core/src/unit_tests/data/object_owner --gas-budget 5000
```
```
----- Publish Results ----
The newly published package object ID: 0x3cfcee192b2fbafbce74a211e40eaf9e4cb746b9
```
Then we create a child object:
```
$ export PKG=0x3cfcee192b2fbafbce74a211e40eaf9e4cb746b9
$ sui client call --package $PKG --module object_owner --function create_child  --gas-budget 1000
```
```
----- Transaction Effects ----
Created Objects:
  - ID: 0xb41d157fdeda968c5b5f0d8b87b6ebb84d7d1941 , Owner: Account Address ( 0x5f67488c28c46e56bcefb808ae499ef323c1236d )
```
At this point we only created the child object, but it's still owned by an address. We can verify that we should be able to call `mutate_child` function by only passing in the child object:
```
$ export CHILD=0xb41d157fdeda968c5b5f0d8b87b6ebb84d7d1941
$ sui client call --package $PKG --module object_owner  --function mutate_child --args $CHILD --gas-budget 1000
```
```
----- Transaction Effects ----
Status : Success
Mutated Objects:
  - ID: 0xb41d157fdeda968c5b5f0d8b87b6ebb84d7d1941
```
Indeed the transasaction succeeded.

Now let's create the `Parent` object as well:
```
$ sui client call --package $PKG --module object_owner --function create_parent --gas-budget 1000
```
```
----- Transaction Effects ----
Created Objects:
  - ID: 0x2f893c18241cfbcd390875f6e1566f4db949392e
```
Now we can make the parent object own the child object:
```
$ export PARENT=0x2f893c18241cfbcd390875f6e1566f4db949392e
$ sui client call --package $PKG --module object_owner --function add_child --args $PARENT $CHILD --gas-budget 1000
```
```
----- Transaction Effects ----
Mutated Objects:
- ID: 0xb41d157fdeda968c5b5f0d8b87b6ebb84d7d1941 , Owner: Object ID: ( 0x2f893c18241cfbcd390875f6e1566f4db949392e )
```
As we can see, the owner of the child object has been changed to the parent object ID.

Now if we try to call `mutate_child` again, we will see an error:
```
$ sui client call --package $PKG --module object_owner  --function mutate_child --args $CHILD --gas-budget 1000
```
```
Object 0xb41d157fdeda968c5b5f0d8b87b6ebb84d7d1941 is owned by object 0x2f893c18241cfbcd390875f6e1566f4db949392e, which is not in the input
```

To be able to mutate the child object, we must also pass the parent object as argument. Hence we need to call the `mutate_child_with_parent` function:
```
$ sui client call --package $PKG --module object_owner  --function mutate_child_with_parent --args $CHILD $PARENT --gas-budget 1000
```
It will finish successfully.

### Transfer Child Objects
In this section, we will introduce a few more APIs that will allow us safely move around child objects.

There are two ways to transfer a child object:
1. Transfer it to an address, thus it will no longer be a child object after the transfer.
2. Transfer it to another object, thus it will still be a child object but with the parent object changed.

#### transfer
First of all, let's look at how to transfer a child object to an address. Recall the `transfer` function as defined in the [transfer module](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/sources/transfer.move):
```rust
```rust
public fun transfer<T: key>(obj: T, recipient: address)
```
`transfer` transfers an object, in this case a child object, to an address.
Using this API is no different than a normal transfer! All that is required is that the object's parent was present as an argument to the initial `entry` function. Let's implement a function that removes a child object from a parent object and transfer it back to the owner:
```rust
public entry fun remove_child(parent: &mut Parent, child: Child, ctx: &mut TxContext) {
    option::extract(&mut parent.child); // this sets the option to none
    transfer::transfer(child, tx_context::sender(ctx));
}
```
In the above function, the `ID` of the child is extracted from the `parent` object. This is not necessary for the code to run, but is necessary to maintain the invariant that all of our `Parent` objects contain a valid `ID` of its child (if it has one). After that, we transfer the `child` like any other object.

#### transfer_to_object
Another way to transfer a child object is to transfer it to another parent. Like we did when transferring to an address, we can use the API we are already familiar with from above:
```rust
public fun transfer_to_object<T: key, R: key>(
    obj: T,
    owner: &mut R,
)
```
After this call, the object `obj` will become a child object of the object `owner`.
Using this API is straight forward, as the only thing special required is that the object's parent was present as an argument to the initial `entry` function.
```rust
public entry fun transfer_child(parent: &mut Parent, child: Child, new_parent: &mut Parent) {
    option::fill(&mut new_parent.child,  option::extract(&mut parent.child));
    transfer::transfer_to_object(child, new_parent);
}
```
Similar to `remove_child`, the `child` object must be passed explicitly by-value in the arguments. First, we extract the existing child `ID` and use it to fill the `child` field of the `new_parent`. Then we use `transfer_to_object` as expected. Note, we can ensure that `new_parent` does not already have a child object. If it did, `option::fill` would abort, thus preventing any change from occurring as a result of this transaction.

### Delete Child Objects
Deleting child objects is similarly straight forward, and no different from deleting normal objects, except that the parent object must be present as an argument to the initial `entry` function.
With this in mind, we can either:
1. First transfer this child object to an account address, which makes this object a regular address-owned object instead of a child object, and hence can be deleted normally.
2. Delete the child object directly

The first case is an application of two concepts already covered, i.e. transferring a child object and deleting an address-owned object. So let's look at the second case, deleting the child directly:
```rust
public entry fun delete_parent_and_child(parent: Parent, child: Child) {
    let Parent { id: parent_id, child: _ } = parent;
    object::delete(parent_id);
    let Child { id } = child;
    object::delete(id);
}
```
After we unpacked the `Parent` object we are able to extract the parent's `id` (bound to `parent_id`) and the ID of the child in the option, `child`. Since `ID` has the `drop` ability, the `Option<ID>` also has the `drop` ability. This means we can discard the value and have done so by not binding it to a local variable with `child: _`. We then also unpack the `child` object to obtain the its `id`. We delete both `UID`s with `object::delete`. Note that these deletions can happen in any order, and Move's type system ensures that both must be deleted by the end of the function, since `UID` does not have `drop`.

### Delete Parent Objects

(This section is still in development)

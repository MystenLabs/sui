## Chapter 1: Object Basics

### Define Sui Object
In Move, besides primitive data types, we can define organized data structures using `struct`. For example:
```rust
struct Color {
    red: u8,
    green: u8,
    blue: u8,
}
```
The above `struct` defines a data structure that can represents RGB color. Structures like this can be useful to organize data with complicated semantics. However, instances of structs like `Color` are not Sui objects yet.
To define a struct that represents a Sui object type, we must add a `key` capability to the definition, and the first field of the struct must be the `id` of the object with type `VersionedID` from the [ID library](../../../../sui_programmability/framework/sources/ID.move):
```rust
use Sui::ID::VersionedID;

struct ColorObject has key {
    id: VersionedID,
    red: u8,
    green: u8,
    blue: u8,
}
```
Now `ColorObject` represents a Sui object type and can be used to create Sui objects that can be eventually stored on the Sui chain.
> :books: In both core Move and Sui Move, the [key ability](https://github.com/diem/move/blob/main/language/documentation/book/src/abilities.md#key) denotes a type that can appear as a key in global storage. However, the structure of global storage is a bit different: core Move uses a (type, `address`)-indexed map, whereas Sui Move uses a map keyed by object ID's.

> :bulb: The `VersionedID` type is internal to Sui and most likely you won't need to deal with it directly. For the curious readers, it contains the unique `ID` of the object and the version of the object. Each time a mutable object is used in a transaction, its version will increase by 1.

### Create Sui Object
Now that we have learned how to define a Sui object type, how do we create/instantiate an Sui object? In order to create a new Sui object from its type, we must assign an initial value to each of the fields, including `id`. The only way to create a new unique `VersionedID` for a Sui object is to call `TxContext::new_id`. The `new_id` function takes the current transaction context as argument to generate unique IDs. The transaction context is of type `&mut TxContext` and should be passed down from an entry function (An [entry function](../move.md#entry-functions) is a function that can be called directly from a transaction.). Let's look at how we may define a constructor for `ColorObject`:
```rust
/// TxContext::Self represents the TxContext module, which allows us call
/// functions in the module, such as the `sender` function.
/// TxContext::TxContext represents the TxContext struct in TxContext module.
use Sui::TxContext::{Self, TxContext};

fun new(red: u8, green: u8, blue: u8, ctx: &mut TxContext): ColorObject {
    ColorObject {
        id: TxContext::new_id(ctx),
        red,
        green,
        blue,
    }
}
```
> :bulb: Move supports *field punning*, which allows us to skip the field values if the field name happens to be the same as the name of the value variable it is bound to. The code above leverages this to write "`red,`" as shorthand for "`red: red,`".

### Store Sui Object
We have defined a constructor for the `ColorObject`. Calling this constructor will put the value in a local variable where it can be returned from the current function, passed to other functions, or stored inside another struct. And of course, the object can be placed in persistent global storage so it can be read by the outside world and accessed in subsequent transactions.

All of the API's for adding objects to persistent storage live in the [`Transfer`](../../../../sui_programmability//framework/sources/Transfer.move) module. One key API is:
```rust
public fun transfer<T: key>(obj: T, recipient: address)
```
which places `obj` in global storage along with metadata that records `recipient` as the owner of the object. In Sui, every object must have an owner, which can be either an account address, another object, or "shared"--see the [ownership](../objects.md#object-ownership) docs for more detail.

> :bulb: In core Move, we would call `move_to<T>(a: address, t: T)` to add the entry `(a, T) -> t` to the global storage. But because (as explained above) the schema of Sui Move's global storage is different, we use the `Transfer` API's instead of `move_to` or the other [global storage operators](https://github.com/diem/move/blob/main/language/documentation/book/src/global-storage-operators.md) in core Move. These operators cannot be used in Sui Move.

A common use of this API is to transfer the object to the sender/signer of the current transaction (e.g., mint an NFT owned by you). The only way to obtain the sender of the current transaction, is to rely on the transaction context passed in from an entry function. The last argument to an entry function must be the current transaction context, defined as `ctx: &mut TxContext`.
To obtain the current signer's address, one can call `TxContext::sender(ctx)`.

Below is the code that creates a new `ColorObject` and make it owned by the sender of the transaction:
```rust
use Sui::Transfer;

// This is an entry function that can be called directly by a Transaction.
public fun create(red: u8, green: u8, blue: u8, ctx: &mut TxContext) {
    let color_object = new(red, green, blue, ctx);
    Transfer::transfer(color_object, TxContext::sender(ctx))
}
```
> :bulb: Naming convention: Constructors are typically named **`new`**, which returns an instance of the struct type. The **`create`** function is typically defined as an entry function, that constructs the struct and transfer it to the desired owner (most commonly the sender).

You can find the full code [here](../../../move_code/objects_tutorial/sources/Ch1/ColorObject.move).

### Onchain Interactions
Now let's try to call the `create` in transactions and see what happens. To do this we need to start Sui and the wallet. Please follow the [Wallet guide](../wallet.md) to start the Sui network and setup the wallet.

Before starting, let's take a look at the default wallet address (this will be the address that will eventually own the object latter):
```
wallet active-address
```
It will tell you the current wallet address.

First of all, we need to publish the code onchain. Assuming the path to the root of the repository is $ROOT:
```
wallet publish --path $ROOT/doc/move_code/objects_tutorial --gas-budget 10000
```
You can find the published package object ID in the **Publish Results**, like this:
```
----- Publish Results ----
The newly published package object: (57258F32746FD1443F2A077C0C6EC03282087C19, SequenceNumber(1), o#b3a8e284dea7482891768e166e4cd16f9749e0fa90eeb0834189016c42327401)
```
Note that the exact data you see will be different. The first hex string in that triple is the package object ID (57258F32746FD1443F2A077C0C6EC03282087C19 in this case).
Next we can call the function to create a color object:
```
wallet call --gas-budget 1000 --package "57258F32746FD1443F2A077C0C6EC03282087C19" --module "Ch1" --function "create" --args 0 255 0
```
In the **Transaction Effects**, you will see an object showing up in the list of **Created Objects**, like this:
```
Created Objects:
5EB2C3E55693282FAA7F5B07CE1C4803E6FDC1BB SequenceNumber(1) o#691b417670979c6c192bdfd643630a125961c71c841a6c7d973cf9429c792efa
```
We can inspect this object and see what kind of object it is:
```
wallet object --id 5EB2C3E55693282FAA7F5B07CE1C4803E6FDC1BB
```
It will show you the meta data of this object with its type:
```
Owner: AddressOwner(k#5db53ebb05fd3ea5f1d163d9d487ee8cd7b591ee)
Version: 1
ID: 5EB2C3E55693282FAA7F5B07CE1C4803E6FDC1BB
Readonly: false
Type: 0x57258f32746fd1443f2a077c0c6ec03282087c19::Ch1::ColorObject
```
As we can see, it's owned by the current default wallet address that we have seen earlier. And the type of this object is `ColorObject`!

You can also look at the data content of the object by adding the `--json` parameter:
```
wallet object --id 5EB2C3E55693282FAA7F5B07CE1C4803E6FDC1BB --json
```
It will print all the value of all the fields in the Move object, such as the value of `red`, `green`, and `blue`.

Congratulations! You have learned how to define, create and transfer objects to an account. In the next chapter, we will learn how to use the objects that we own.
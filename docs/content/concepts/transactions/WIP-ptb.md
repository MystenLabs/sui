# Programmable Transactions Blocks

Programmable transaction blocks are used to define all user transactions on Sui. These transactions allow a user to call multiple Move functions, manage their objects, and manage their coins in a single transaction--without publishing a new Move module! Additionally, the structure of programmable transaction blocks was designed with automation and transaction builders in mind. In other words, they are designed to be a lightweight and flexible way of generating transactions. That being said, more intricate programming patterns, such as loops, are not supported, and in those cases, a new Move module should be published.

Each programmable transaction block is consists of a block that is comprised of individual transaction commands (sometimes referred to themselves as transactions). Each transaction command is executed in order, and the results from a transaction command can be used in any subsequent transaction command. The effects, i.e. object modifications or transfers, of all transaction commands in a block are applied atomically at the end of the transaction, and if one transaction command fails, the entire block fails and no effects from the commands are applied.

This document will cover the semantics of the execution of the transaction commands.

## Transaction Type

In this document, we will be looking at the two parts of a programmable transaction block that are relevant to the exectuion semantics. Other transaction information, such as the transaction sender or the gas limit, might be referenced but are not out of scope. The programmable transaction block consists of two components

- The inputs, `Vec<CallArg>`, is a vector of arguments, either objects or pure values, that can be used in the transaction commands. The objects are either owned by the sender or are shared/immutable objects. The pure values represent simple Move values, such as `u64` or `String` values, which can be constructed purely by their bytes.
- The commands, `Vec<Command>`, is a vector of transaction commands. The possible commands are:
  - `MoveCall` invokes either an `entry` or a `public` Move function in a published package.
  - `TransferObjects` sends multiple (1 or more) objects to a specified address.
  - `SplitCoins` splits off mutliple (1 or more) coins from a single coin. It can be any `sui::coin::Coin` object.
  - `MergeCoins` merges multiple (1 or more) coins into a single coin. Any `sui::coin::Coin` objects can be merged, as long as they are all of the same type.
  - `MakeMoveVec` creates a vector (potentially empty) of Move values. This is used primiarly to construct vectors of Move values to be used as arguments to `MoveCall`.
  - `Publish` creates a new package and calls the `init` function of each module in the package.
  - `Upgrade` upgrades an existing package. The upgrade is gated by the `sui::package::UpgradeCap` for that package.

## Arguments and Results

Inputs and Results are the two types of values that can be used in transaction commands. Inputs are the values that are provided to the transaction block, and results are the values that are produced by the transaction block's commands. The inputs are either objects or simple Move values, and the results are arbitrary Move values (including objects).

The inputs and results can be seen as populating an array of values. For inputs, there is a single array, but for results, there is an array for each individual transaction command, creating a 2D-array of result values. These values can be accessed by borrowing (mutably or immutably), by copying (if the type permits), or by moving (which takes the value out of the array without re-indexing). First, we will look at the shape of each array, and then we will look at the semantics of each access type.

### Inputs

Input arguments to a programmable transaction block are broadly categorized as either objects or pure values. The direct implementation of these arguments is often obscured by transaction builders or SDKs. This section will describe information or data needed by the Sui network when specifying the list of inputs, `Vec<CallArg>`. Where each `CallArg` is either an object, `CallArg::Object(ObjectArg)`, which contains the necessary metadata to specify to object being used, or a pure value, `CallArg::Pure(PureArg)`, which contains the bytes of the value.

For object inputs, there is a different set of metadata needed differs depending on the type ownership of the object. The rules for authentication of these objects is described elsewhere (TODO LINK), but below is the actual data in the `ObjectArg` enum.

- If the object is owned by an address (or it is immutable), then `ObjectArg::ImmOrOwnedObject(ObjectRef)` is used. The `ObjectRef` is a triple `(ObjectID, SequenceNumber, ObjectDigest)` which respectively specifies the object's ID, it's version or sequence number, and the digest of the object's data.
- If an object is shared, then `Object::SharedObject { id: ObjectID, initial_shared_version: SequenceNumber, mutable: bool }` is used. Unlike `ImmOrOwnedObject`, a shared's objects version and digest are determined by the network's consensus protocol. The `initial_shared_version` is the version of the object when it was first shared, which is used by consensus when it has not yet seen a transaction with that object. While all shared objects _can_ be mutated, the `mutable` flag indicates whether the object will be used mutably in this transaction. In the case where the `mutable` flag is set to `false`, the object is read-only, and the system can schedule other read-only transactions in parallel.
- If the object is owned by another object, i.e. it was sent to an object's ID via the `TransferObjects` command or the `sui::transfer::transfer` function, then `ObjectArg::Receiving(ObjectRef)` is used. The data in the `ObjectRef` is the same as for the `ImmOrOwnedObject` case.

For pure inputs, the only data provided is the BCS (TODO Link) bytes. The bytes are not validated until the type is specified in a transaction command, e.g. in `MoveCall` or `MakeMoveVec`. Not all Move values can be constructed from BCS bytes. The following types are supported:

- All primitive types:
  - `u8`
  - `u16`
  - `u32`
  - `u64`
  - `u128`
  - `u256`
  - `bool`
  - `address`
- A string, either an ASCII string `std::ascii::String` or UTF8 string `std::string::String`. In either case, the bytes will be validated to be a valid string with the respective encoding.
- An object ID `sui::object::ID`.
- A vector, `vector<T>`, where `T` is a valid type for a pure input, checked recursively.
- An option, `std::option::Option<T>`, where `T` is a valid type for a pure input, checked recursively.

### Results

Each transaction command produces a (possibly empty) array of values. The type of the value can be any arbitrary Move type, so unlike inputs, the values are not limited to objects or pure values. The number of results generated and their types is specific to each transaction command. The specifics for each command can be found in the section for that command, but in summary:

- `MoveCall`: the number of results and their types are determined by the Move function being called. Note that Move functions that return references are not supported at this time.
- `SplitCoins`: produces (1 or more) coins from a single coin. The type of each coin is `sui::coin::Coin<T>` where the specific coin type `T` matches the coin being split.
- `Publish`: returns the upgrade capability, `sui::package::UpgradeCap` for the newly published package.
- `TransferObjects`, `MergeCoins`, and `Publish` do not produce any results (an empty result vector).

### Argument Structure and Usage

Each command takes `Argument`s, which specify the input or result being used. The usage (by-reference or by-value) is inferred based on the type of the argument and the expected argument of the command. First, let's look at the structure of the `Argument` enum.

- `Input(u16)` is an input argument, where the `u16` is the index of the input in the input vector.
  - For example, given an input vector of `[Object1, Object2, Pure1, Object3]`, `Object1` would be accessed with `Input(0)` and `Pure1` would be accessed with `Input(2)`.
- `GasCoin` is a special input argument representing the object for the GasCoin. It is kept separate from the other inputs because the gas coin is always present in each transaction and has special restrictions not present for other inputs. Additionally, the gas coin being separate makes its usage very explicit, which is helpful for sponsored transactions where the sponsor might not want the sender to use the gas coin for anything other than gas.
  - The gas coin cannot be taken by-value except with the `TransferObjects` command. If you need an owned version of the gas coin, you can first use `SplitCoins` to split off a single coin.
- `NestedResult(u16, u16)` uses the value from a previous command. The first `u16` is the index of the command in the command vector, and the second `u16` is the index of the result in the result vector of that command.
  - For example, given a command vector of `[MoveCall1, MoveCall2, TransferObjects]` where `MoveCall2` has a result vector of `[Value1, Value2]`, `Value1` would be accessed with `NestedResult(1, 0)` and `Value2` would be accessed with `NestedResult(1, 1)``.
- `Result(u16)` is a special form of `NestedResult` where `Result(i)` is equivalent to `NestedResult(i, 0)`. However, this will error if the result array at index `i` is empty or has more than one value.
  - The intention of `Result` was to allow for accessing of the entire result array, but that is not yet supported at this time. So in general, `NestedResult` should be used instead of `Result`.

## Commands

### `MoveCall`

### `TransferObjects`

### `SplitCoins`

### `MergeCoins`

### `MakeMoveVec`

### `Publish`

### `Upgrade`

## Execution

### Start of Execution

### Executing a Command

### End of Execution

## Examples
